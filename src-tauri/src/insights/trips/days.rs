//! Where the user was each day, and the bases they lived at.
//!
//! Evidence is grouped into areas: each piece joins the nearest area within `AREA_KM`, strongest
//! first, and areas are then centred on their medians. A day belongs to the area its evidence
//! weighs most. An area is a base while the user keeps coming back to it: at least `BASE_DAYS`
//! days across at least `BASE_SPAN` days, never more than `BASE_GAP` days apart.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};

use super::reservations::Reservation;
use crate::finance_contract::Transaction;
use crate::numeric::median;
use crate::places::Place;

pub const AREA_KM: f64 = 100.0;
const BASE_SPAN: i64 = 28;
const BASE_DAYS: usize = 8;
const BASE_GAP: i64 = 150;
/// Flights farther apart than this are separate journeys, not a stay between them.
const STAY_DAYS: i64 = 30;

/// A place named by a piece of evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Named<'a> {
    pub city: Option<&'a str>,
    pub region: Option<&'a str>,
    pub country: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub enum Source<'a> {
    Purchase(&'a Transaction),
    /// A flight or stay in the user's calendar.
    Reservation,
}

#[derive(Clone, Copy, Debug)]
pub struct Evidence<'a> {
    pub date: NaiveDate,
    pub point: (f64, f64),
    pub named: Named<'a>,
    pub weight: u32,
    pub source: Source<'a>,
}

impl Evidence<'_> {
    /// Placed by the phone or a reservation, rather than a merchant's printed city.
    pub fn strong(&self) -> bool {
        self.weight > 1
    }
}

/// Great-circle distance in kilometres.
pub fn distance_km(left: (f64, f64), right: (f64, f64)) -> f64 {
    let (lat1, lon1) = (left.0.to_radians(), left.1.to_radians());
    let (lat2, lon2) = (right.0.to_radians(), right.1.to_radians());
    let a = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * 6_371.0 * a.sqrt().asin()
}

pub fn purchase<'a>(
    date: NaiveDate,
    transaction: &'a Transaction,
    place: &'a Place,
) -> Option<Evidence<'a>> {
    Some(Evidence {
        date,
        point: place.coordinates()?,
        named: Named {
            city: place.city.as_deref(),
            region: place.region.as_deref(),
            country: &place.country,
        },
        weight: if place.source == crate::places::PlaceSource::Device {
            3
        } else {
            1
        },
        source: Source::Purchase(transaction),
    })
}

fn at(date: NaiveDate, city: &'static crate::places::City) -> Evidence<'static> {
    Evidence {
        date,
        point: (city.lat, city.lon),
        named: Named {
            city: Some(city.name.as_str()),
            region: Some(city.admin1.as_str()).filter(|region| !region.is_empty()),
            country: &city.country,
        },
        weight: 2,
        source: Source::Reservation,
    }
}

/// A flight puts the user at its origin the day it leaves and at its destination the day it
/// lands; a stay, in its town every night. Of the towns an address could name, the stay takes
/// the one nearest the other evidence of those days.
pub fn reservations<'a>(
    reservations: &[Reservation],
    others: &[Evidence<'a>],
) -> Vec<Evidence<'a>> {
    let mut evidence = Vec::new();
    for reservation in reservations {
        if let Reservation::Flight {
            departs,
            arrives,
            from,
            to,
        } = reservation
        {
            evidence.extend(from.map(|from| at(*departs, from)));
            evidence.push(at(*arrives, to));
        }
    }
    for reservation in reservations {
        let Reservation::Stay {
            from,
            until,
            cities,
        } = reservation
        else {
            continue;
        };
        let nearby = others
            .iter()
            .chain(&evidence)
            .filter(|other| (other.date - *from).num_days().abs() <= 3)
            .map(|other| other.point)
            .collect::<Vec<_>>();
        let closest = |city: &&crate::places::City| {
            nearby
                .iter()
                .map(|point| distance_km(*point, (city.lat, city.lon)))
                .fold(f64::INFINITY, f64::min)
        };
        let city = if nearby.is_empty() {
            cities.first().copied()
        } else {
            cities
                .iter()
                .min_by(|left, right| closest(left).total_cmp(&closest(right)))
                .copied()
        };
        let Some(city) = city else {
            continue;
        };
        let nights = (*until - *from).num_days().max(1);
        for night in 0..nights {
            evidence.push(at(*from + Duration::days(night), city));
        }
    }
    evidence
}

pub struct Areas {
    pub centres: Vec<(f64, f64)>,
    /// The area of each piece of evidence, in the order given.
    pub of: Vec<usize>,
}

impl Areas {
    pub fn nearest(&self, point: (f64, f64)) -> Option<usize> {
        (0..self.centres.len()).min_by(|&left, &right| {
            distance_km(self.centres[left], point)
                .total_cmp(&distance_km(self.centres[right], point))
        })
    }
}

pub fn areas(evidence: &[Evidence<'_>]) -> Areas {
    let mut order = (0..evidence.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| {
        (
            std::cmp::Reverse(evidence[index].weight),
            evidence[index].date,
            index,
        )
    });
    let mut seeds: Vec<(f64, f64)> = Vec::new();
    let mut members: Vec<Vec<(f64, f64)>> = Vec::new();
    for index in order {
        let point = evidence[index].point;
        let near = (0..seeds.len())
            .map(|area| (area, distance_km(seeds[area], point)))
            .filter(|(_, distance)| *distance <= AREA_KM)
            .min_by(|left, right| left.1.total_cmp(&right.1));
        match near {
            Some((area, _)) => members[area].push(point),
            None => {
                seeds.push(point);
                members.push(vec![point]);
            }
        }
    }
    let centres = members
        .into_iter()
        .map(|points| {
            let (mut lats, mut lons): (Vec<f64>, Vec<f64>) = points.into_iter().unzip();
            (
                median(&mut lats).unwrap_or_default(),
                median(&mut lons).unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    let mut areas = Areas {
        centres,
        of: Vec::new(),
    };
    areas.of = evidence
        .iter()
        .map(|piece| areas.nearest(piece.point).unwrap_or_default())
        .collect();
    areas
}

/// Each day with evidence and the area it weighs most toward; the lower area on a tie.
pub fn days(evidence: &[Evidence<'_>], areas: &Areas) -> BTreeMap<NaiveDate, usize> {
    let mut weights = BTreeMap::<(NaiveDate, usize), u32>::new();
    for (piece, &area) in evidence.iter().zip(&areas.of) {
        *weights.entry((piece.date, area)).or_default() += piece.weight;
    }
    let mut days = BTreeMap::<NaiveDate, (usize, u32)>::new();
    for ((date, area), weight) in weights {
        let best = days.entry(date).or_insert((area, weight));
        if weight > best.1 {
            *best = (area, weight);
        }
    }
    days.into_iter()
        .map(|(date, (area, _))| (date, area))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Base {
    pub area: usize,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

pub fn bases(days: &BTreeMap<NaiveDate, usize>) -> Vec<Base> {
    let mut by_area = BTreeMap::<usize, Vec<NaiveDate>>::new();
    for (date, area) in days {
        by_area.entry(*area).or_default().push(*date);
    }
    let mut bases = Vec::new();
    for (area, dates) in by_area {
        let mut segment: Vec<NaiveDate> = Vec::new();
        for date in dates.into_iter().map(Some).chain([None]) {
            let ends = segment
                .last()
                .is_some_and(|last| date.is_none_or(|date| (date - *last).num_days() > BASE_GAP));
            if ends {
                let (first, last) = (segment[0], segment[segment.len() - 1]);
                if (last - first).num_days() + 1 >= BASE_SPAN && segment.len() >= BASE_DAYS {
                    bases.push(Base {
                        area,
                        start: first,
                        end: last,
                    });
                }
                segment.clear();
            }
            segment.extend(date);
        }
    }
    bases.sort_by_key(|base| (base.start, base.area));
    bases
}

pub fn at_base(bases: &[Base], date: NaiveDate, area: usize) -> bool {
    bases
        .iter()
        .any(|base| base.area == area && base.start <= date && date <= base.end)
}

/// A flight between a base and somewhere away belongs to the trip it starts or ends, from the
/// day it leaves to the day it lands. After a flight home, the user is home the next day
/// unless something says otherwise.
pub fn flight_days(
    reservations: &[Reservation],
    areas: &Areas,
    bases: &[Base],
    days: &mut BTreeMap<NaiveDate, usize>,
) {
    for reservation in reservations {
        let Reservation::Flight {
            departs,
            arrives,
            from: Some(from),
            to,
        } = reservation
        else {
            continue;
        };
        let (Some(origin), Some(destination)) = (
            areas.nearest((from.lat, from.lon)),
            areas.nearest((to.lat, to.lon)),
        ) else {
            continue;
        };
        let away = match (
            at_base(bases, *departs, origin),
            at_base(bases, *arrives, destination),
        ) {
            (true, false) => destination,
            (false, true) => {
                days.entry(*arrives + Duration::days(1))
                    .or_insert(destination);
                origin
            }
            _ => continue,
        };
        for offset in 0..=(*arrives - *departs).num_days() {
            days.insert(*departs + Duration::days(offset), away);
        }
    }
}

/// Between landing away from a base and the next flight out of the same place, the user was
/// there, though nothing was bought. Fills only days without evidence of their own.
pub fn fill_stays(
    reservations: &[Reservation],
    areas: &Areas,
    bases: &[Base],
    days: &mut BTreeMap<NaiveDate, usize>,
) {
    let mut flights = reservations
        .iter()
        .filter_map(|reservation| match reservation {
            Reservation::Flight {
                departs,
                arrives,
                from,
                to,
            } => Some((*departs, *arrives, *from, *to)),
            Reservation::Stay { .. } => None,
        })
        .collect::<Vec<_>>();
    flights.sort_by_key(|flight| flight.0);
    for pair in flights.windows(2) {
        let ((_, landed, _, there), (left, _, Some(from), _)) = (pair[0], pair[1]) else {
            continue;
        };
        let point = (there.lat, there.lon);
        let same_place = distance_km(point, (from.lat, from.lon)) <= AREA_KM;
        let span = (left - landed).num_days();
        let Some(area) = areas.nearest(point) else {
            continue;
        };
        if !same_place || span <= 1 || span > STAY_DAYS || at_base(bases, landed, area) {
            continue;
        }
        for offset in 1..span {
            days.entry(landed + Duration::days(offset)).or_insert(area);
        }
    }
}

/// Runs of days away from every base active at the time: a day at a base ends one, and so does
/// more than `gap` days without evidence. Returns each run's days and whether it ended at a base.
pub fn away_runs(
    days: &BTreeMap<NaiveDate, usize>,
    bases: &[Base],
    gap: i64,
) -> Vec<(BTreeSet<NaiveDate>, bool)> {
    let mut runs = Vec::new();
    let mut current = BTreeSet::<NaiveDate>::new();
    for (&date, &area) in days {
        if at_base(bases, date, area) {
            if !current.is_empty() {
                runs.push((std::mem::take(&mut current), true));
            }
            continue;
        }
        if current
            .last()
            .is_some_and(|last| (date - *last).num_days() > gap)
        {
            runs.push((std::mem::take(&mut current), false));
        }
        current.insert(date);
    }
    if !current.is_empty() {
        runs.push((current, false));
    }
    runs
}
