"""Builds the city gazetteer Brief reads place names from bank descriptors with.

Sources, licensed CC BY 4.0: GeoNames cities1000
(https://download.geonames.org/export/dump/cities1000.zip) and, optionally, the full dump
(https://download.geonames.org/export/dump/allCountries.zip). Usage:

    python3 -I scripts/build-places.py path/to/cities1000.txt [path/to/allCountries.txt]

Each row: display name, country, admin1 code, latitude, longitude, population, then the
spellings a descriptor may use, normalized as `places.rs` normalizes descriptors. Big cities keep
their alternate names (MUENCHEN, ZUERICH); every city gets its ASCII name and a German
transliteration. With the full dump, populated places GeoNames lists without a population are
added when they are also a municipality or borough of the same name in the same region (a town
merged into a city, such as Lachine in Montreal), with a population of 0. The output is
byte-for-byte reproducible.
"""

import gzip
import sys
import unicodedata
from pathlib import Path

OUTPUT = Path(__file__).resolve().parent.parent / "src-tauri/src/places/cities.tsv.gz"
ALTERNATES_FROM = 100_000
GERMAN = str.maketrans({"ä": "ae", "ö": "oe", "ü": "ue", "Ä": "Ae", "Ö": "Oe", "Ü": "Ue", "ß": "ss"})
MUNICIPALITIES = {"ADM3", "ADM4", "ADM5"}


def normalize(text: str) -> str:
    text = unicodedata.normalize("NFKD", text)
    text = "".join(c for c in text if not unicodedata.combining(c)).upper()
    return " ".join("".join(c if c.isalnum() else " " for c in text).split())


def row(field: list[str], population: int) -> str | None:
    name, ascii_name, alternates = field[1], field[2], field[3]
    spellings = {normalize(name), normalize(ascii_name), normalize(name.translate(GERMAN))}
    if population >= ALTERNATES_FROM:
        spellings |= {normalize(alternate) for alternate in alternates.split(",") if alternate}
    spellings = sorted(s for s in spellings if 4 <= len(s) <= 30 and s.isascii())
    if not spellings or "\t" in name:
        return None
    return "\t".join(
        [
            name,
            field[8],
            field[10],
            f"{float(field[4]):.3f}",
            f"{float(field[5]):.3f}",
            str(population),
            "|".join(spellings),
        ]
    )


def unpopulated_municipalities(source: Path, known: set[str]) -> list[str]:
    """Populated places without a population that share their name with a municipality."""
    municipalities = set()
    places = {}
    with source.open(encoding="utf-8") as handle:
        for line in handle:
            field = line.rstrip("\n").split("\t")
            key = (field[8], field[10], field[1])
            if field[6] == "A" and field[7] in MUNICIPALITIES:
                municipalities.add(key)
            elif (
                field[6] == "P"
                and (field[7] in {"PPL", "PPLX"} or field[7].startswith("PPLA"))
                and int(field[14] or 0) == 0
                and field[0] not in known
            ):
                places.setdefault(key, field)
    rows = []
    for key, field in places.items():
        if key in municipalities:
            built = row(field, 0)
            if built:
                rows.append(built)
    return rows


def main(source: Path, everything: Path | None) -> None:
    rows = []
    known = set()
    with source.open(encoding="utf-8") as handle:
        for line in handle:
            field = line.rstrip("\n").split("\t")
            known.add(field[0])
            built = row(field, int(field[14] or 0))
            if built:
                rows.append(built)
    if everything:
        rows += unpopulated_municipalities(everything, known)
    rows.sort()
    data = ("\n".join(rows) + "\n").encode("utf-8")
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    with OUTPUT.open("wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", mtime=0, compresslevel=9) as out:
        out.write(data)
    print(f"{len(rows)} places, {OUTPUT.stat().st_size / 1e6:.2f} MB -> {OUTPUT}")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]) if len(sys.argv) > 2 else None)
