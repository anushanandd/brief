use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde_json::json;

use super::*;
use crate::storage::Annotation;
use crate::transaction_policy::apply;

fn person(id: &str, name: &str, match_names: &[&str]) -> Person {
    Person {
        id: format!("person:{id}"),
        name: name.into(),
        contact_id: None,
        match_names: match_names.iter().map(|name| (*name).into()).collect(),
    }
}

fn transaction(id: &str, date: &str, merchant: &str, category: &str, amount: f64) -> Transaction {
    serde_json::from_value(json!({
        "id": id, "merchant": merchant, "category": category, "date": date, "amount": amount,
        "account": "Checking", "accountId": "plaid:checking", "pending": false,
        "description": null
    }))
    .unwrap()
}

/// Classified and annotated as a read applies them, then matched.
fn read(
    mut transactions: Vec<Transaction>,
    annotations: &BTreeMap<String, Annotation>,
    people: &[Person],
) -> Vec<Transaction> {
    apply(&mut transactions, std::iter::empty(), annotations);
    assign(&mut transactions, people);
    transactions
}

fn sample_people() -> Vec<Person> {
    vec![
        person("a", "Example Person A", &["EXAMPLE PERSON A"]),
        person("b", "Example Person B", &["EXAMPLE PERSON B", "PERSON B"]),
    ]
}

#[test]
fn matches_a_payment_to_a_person_by_whole_match_name() {
    let people = sample_people();
    let read = read(
        vec![
            transaction(
                "sent",
                "2026-09-01",
                "Zelle payment to EXAMPLE PERSON A Conf# x1y2",
                "Transfer Out",
                -40.0,
            ),
            transaction(
                "received",
                "2026-09-02",
                "ZELLE FROM PERSON B ON 09/02 REF # 99",
                "Transfer In",
                25.0,
            ),
        ],
        &BTreeMap::new(),
        &people,
    );
    assert_eq!(read[0].people, ["person:a"]);
    assert_eq!(read[0].people_source, Some(PeopleSource::Matched));
    assert_eq!(read[1].people, ["person:b"]);
}

#[test]
fn never_matches_a_purchase_or_part_of_a_word() {
    let people = vec![person("a", "Example", &["EXAMPLE"])];
    let read = read(
        vec![
            transaction("cafe", "2026-09-01", "Example Cafe", "Food And Drink", -9.0),
            transaction(
                "zelle",
                "2026-09-01",
                "Zelle payment to EXAMPLES INC",
                "Transfer Out",
                -9.0,
            ),
        ],
        &BTreeMap::new(),
        &people,
    );
    assert!(read
        .iter()
        .all(|t| t.people.is_empty() && t.people_source.is_none()));
}

#[test]
fn the_longest_match_wins_and_a_tie_tags_nobody() {
    let people = vec![
        person("short", "Person", &["PERSON"]),
        person("long", "Example Person", &["EXAMPLE PERSON"]),
        person("twin", "Example Person Twin", &["EXAMPLE PERSON"]),
    ];
    let longest = read(
        vec![transaction(
            "t",
            "2026-09-01",
            "Venmo EXAMPLE PERSON",
            "Transfer Out",
            -5.0,
        )],
        &BTreeMap::new(),
        &people[..2],
    );
    assert_eq!(longest[0].people, ["person:long"]);
    let tied = read(
        vec![transaction(
            "t",
            "2026-09-01",
            "Venmo EXAMPLE PERSON",
            "Transfer Out",
            -5.0,
        )],
        &BTreeMap::new(),
        &people,
    );
    assert!(tied[0].people.is_empty());
}

#[test]
fn a_hand_choice_stands_even_when_it_is_nobody() {
    let people = sample_people();
    let annotations = BTreeMap::from([
        (
            "nobody".to_owned(),
            Annotation {
                people: Some(Vec::new()),
                ..Default::default()
            },
        ),
        (
            "chosen".to_owned(),
            Annotation {
                people: Some(vec!["person:b".into(), "person:deleted".into()]),
                ..Default::default()
            },
        ),
    ]);
    let read = read(
        vec![
            transaction(
                "nobody",
                "2026-09-01",
                "Zelle payment to EXAMPLE PERSON A",
                "Transfer Out",
                -40.0,
            ),
            transaction("chosen", "2026-09-01", "Dinner", "Food And Drink", -80.0),
        ],
        &annotations,
        &people,
    );
    assert!(read[0].people.is_empty());
    assert_eq!(read[0].people_source, Some(PeopleSource::Manual));
    assert_eq!(read[1].people, ["person:b"]);
    assert_eq!(read[1].people_source, Some(PeopleSource::Manual));
}

#[test]
fn a_posted_transaction_keeps_people_chosen_while_pending() {
    let mut posted = transaction("posted", "2026-09-02", "Dinner", "Food And Drink", -80.0);
    posted.pending_transaction_id = Some("pending".into());
    let annotations = BTreeMap::from([(
        "pending".to_owned(),
        Annotation {
            people: Some(vec!["person:a".into()]),
            ..Default::default()
        },
    )]);
    let read = read(vec![posted], &annotations, &sample_people());
    assert_eq!(read[0].people, ["person:a"]);
}

#[test]
fn reads_the_name_from_each_bank_zelle_format() {
    for (descriptor, name) in [
        (
            "Zelle payment to EXAMPLE PERSON Conf# abc123",
            Some("EXAMPLE PERSON"),
        ),
        (
            "Zelle Zelle Payment From Example Person JPM99ab1",
            Some("EXAMPLE PERSON"),
        ),
        (
            "ZELLE TO PERSON EXAMPLE ON 09/01 REF # PP0X",
            Some("PERSON EXAMPLE"),
        ),
        ("Zelle Transfer Conf# 12345", None),
        // A statement import's page text ran on after the name.
        (
            "Zelle Payment From Example Ly Person *start*dre portrait disclosure",
            Some("EXAMPLE LY PERSON"),
        ),
        (
            "Zelle payment from O'Example Person-Name",
            Some("O EXAMPLE PERSON NAME"),
        ),
        ("Zelle to A 0001", None),
    ] {
        assert_eq!(zelle_name(descriptor).as_deref(), name, "{descriptor}");
    }
}

#[test]
fn summarizes_people_and_offers_unmatched_zelle_names() {
    let people = sample_people();
    let annotations = BTreeMap::from([(
        "dinner".to_owned(),
        Annotation {
            people: Some(vec!["person:a".into()]),
            ..Default::default()
        },
    )]);
    let transactions = read(
        vec![
            transaction(
                "sent",
                "2026-09-01",
                "Zelle payment to EXAMPLE PERSON A",
                "Transfer Out",
                -40.10,
            ),
            transaction(
                "back",
                "2026-09-05",
                "Zelle payment from EXAMPLE PERSON A",
                "Transfer In",
                12.05,
            ),
            transaction("dinner", "2026-09-03", "Dinner", "Food And Drink", -80.0),
            transaction(
                "new-1",
                "2026-09-04",
                "Zelle payment to EXAMPLE PERSON C",
                "Transfer Out",
                -5.0,
            ),
            transaction(
                "new-2",
                "2026-09-06",
                "Zelle payment from EXAMPLE PERSON C",
                "Transfer In",
                5.0,
            ),
        ],
        &annotations,
        &people,
    );
    let summary = super::people(&transactions, &people, &[]);
    let a = &summary.people[0];
    assert_eq!(a.id, "person:a");
    assert_eq!(a.transaction_count, 3);
    // A dinner together is not money sent to them.
    assert_eq!(a.sent, -40.10);
    assert_eq!(a.received, 12.05);
    assert_eq!(a.first, NaiveDate::from_ymd_opt(2026, 9, 1));
    assert_eq!(a.last, NaiveDate::from_ymd_opt(2026, 9, 5));
    assert_eq!(summary.people[1].transaction_count, 0);
    // Only what went to people they know adds up.
    assert_eq!(summary.sent, 40.10);
    assert_eq!(
        summary.suggestions,
        [NameSuggestion {
            name: "Example Person C".into(),
            match_name: "EXAMPLE PERSON C".into(),
            transaction_ids: vec!["new-2".into(), "new-1".into()],
            // Two of three words in common is someone else with the same family name.
            matches: Vec::new(),
        }]
    );
}

#[test]
fn a_name_in_payments_fits_people_by_part_of_their_name() {
    let fit = |paid: &str, name: &str| name_fit(paid, name);
    // Every word of their name is in it, middle name and all.
    assert_eq!(
        fit("EXAMPLE LY PERSON", "Example Person"),
        Some(Fit::Likely)
    );
    assert_eq!(
        fit("EXAMPLE PERSON", "Example Q. Person"),
        Some(Fit::Likely)
    );
    // A first name alone could be them.
    assert_eq!(fit("EXAMPLE", "Example Person"), Some(Fit::Possible));
    // The same last name and a first name that starts the same way: Edmund and Eddie.
    assert_eq!(fit("EDMUND LY PERSON", "Eddie Person"), Some(Fit::Possible));
    // Otherwise, no.
    assert_eq!(fit("EXAMPLE PERSON", "Other Person"), None);
    assert_eq!(fit("EXAMPLE PERSON C", "Example Person A"), None);
    assert_eq!(fit("PERSON", "Example Person"), None);

    let people = vec![
        person("full", "Example Person", &["EXAMPLE PERSON"]),
        person("nick", "Exa Other", &[]),
        person("named", "Other Name", &["EXAMPLE"]),
    ];
    let summary = super::people(
        &read(
            vec![transaction(
                "t",
                "2026-09-01",
                "Zelle payment from EXAMPLE OTHER Conf# 1",
                "Transfer In",
                5.0,
            )],
            &BTreeMap::new(),
            &[],
        ),
        &people,
        &[],
    );
    assert_eq!(
        summary.suggestions[0].matches,
        // Equally possible, they follow by name; a match name counts as a name of theirs.
        [
            PersonFit {
                id: "person:nick".into(),
                fit: Fit::Possible,
            },
            PersonFit {
                id: "person:named".into(),
                fit: Fit::Possible,
            },
        ]
    );
}

#[test]
fn companions_follow_a_trip_as_it_grows_or_splits() {
    let trip = |id: &str, transactions: &[&str]| Trip {
        id: id.into(),
        start: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
        days: Vec::new(),
        per_day: 0.0,
        destination: "Example City".into(),
        abroad: false,
        total: 0.0,
        categories: Vec::new(),
        rest: 0.0,
        rest_share: 0.0,
        transaction_ids: transactions.iter().map(|id| (*id).into()).collect(),
        ongoing: false,
        flew: false,
        stops: Vec::new(),
        companions: Vec::new(),
    };
    let people = sample_people();
    let saved = [TripCompanions {
        transaction_ids: vec!["t1".into(), "t2".into()],
        people: vec!["person:a".into(), "person:deleted".into()],
    }];
    let mut trips = [
        trip("grown", &["t1", "t2", "t3"]),
        trip("other half", &["t2"]),
        trip("unrelated", &["t9"]),
    ];
    trip_companions(&mut trips, &saved, &people);
    assert_eq!(trips[0].companions, ["person:a"]);
    assert_eq!(trips[1].companions, ["person:a"]);
    assert!(trips[2].companions.is_empty());
    let summary = super::people(&[], &people, &trips);
    let a = summary.people.iter().find(|p| p.id == "person:a").unwrap();
    assert_eq!(a.trip_ids, ["grown", "other half"]);
}
