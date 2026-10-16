use super::read::{file_hints, money, FileHints, Line, Page, Reading};
use super::*;

fn line(text: &str, x: f64, y: f64, w: f64) -> Line {
    Line {
        text: text.into(),
        x,
        y,
        w,
        h: 0.01,
        confidence: 1.0,
    }
}

fn vision(lines: Vec<Line>) -> Reading {
    Reading {
        pages: vec![Page {
            source: "vision".into(),
            lines,
        }],
        encrypted: false,
    }
}

#[test]
fn reads_amounts_as_forms_print_them() {
    assert_eq!(money("4,807.69"), Some(4_807.69));
    assert_eq!(money("$1,056.00"), Some(1_056.0));
    assert_eq!(money("4,342.05-"), Some(-4_342.05));
    assert_eq!(money("(12.00)"), Some(-12.0));
    assert_eq!(money("-40.00"), Some(-40.0));
    // Box numbers, hours without cents, and identifiers aren't amounts.
    assert_eq!(money("12"), None);
    assert_eq!(money("2025"), None);
    assert_eq!(money("1041235821296"), None);
}

#[test]
fn a_file_name_suggests_the_year_and_issuer() {
    assert_eq!(
        file_hints("W-2 2025 Forge Global.jpeg"),
        FileHints {
            year: Some(2025),
            issuer: Some("Forge Global".into()),
            mask: None,
        }
    );
    assert_eq!(file_hints("Payslip_2026-05-08.pdf").issuer, None);
    // An institution's own name for a statement carries the account's last four digits.
    let chase = file_hints("20250430-statements-9967-.pdf");
    assert_eq!((chase.mask.as_deref(), chase.issuer), (Some("9967"), None));
}

/// Two copies of a W-2, as a printout carries them; the second copy's box 1 is misread.
fn w2() -> Reading {
    let mut lines = Vec::new();
    for (top, wages) in [(0.1, "14,316.06"), (0.5, "14,316.66")] {
        lines.extend([
            line(
                "Form W-2 Wage and Tax Statement 2025",
                0.05,
                top - 0.05,
                0.4,
            ),
            line("c Employer's name, address, and ZIP code", 0.05, top, 0.3),
            line("EXAMPLE", 0.05, top + 0.02, 0.06),
            line("GLOBAL INC", 0.12, top + 0.02, 0.1),
            line("1 Wages, tips, other compensation", 0.5, top, 0.18),
            line(wages, 0.52, top + 0.02, 0.08),
            line("2 Federal income tax withheld", 0.72, top, 0.18),
            line("1,433.04", 0.74, top + 0.02, 0.07),
            line("3 Social security wages", 0.5, top + 0.06, 0.15),
            line("14316.06", 0.52, top + 0.08, 0.08),
            line("4 Social security tax withheld", 0.72, top + 0.06, 0.18),
            line("887.60", 0.74, top + 0.08, 0.06),
            line("12a See instructions for box 12", 0.72, top + 0.14, 0.18),
            line("D 1,298.06", 0.73, top + 0.16, 0.1),
            line("CA 030-48212 14316.06", 0.05, top + 0.2, 0.3),
        ]);
    }
    vision(lines)
}

#[test]
fn a_w2_reads_each_box_and_flags_copies_that_disagree() {
    let read = understand(&w2(), "W-2 2025 Example Global.jpeg");
    assert_eq!(read.kind, Some(Kind::TaxForm));
    assert_eq!(read.form.as_deref(), Some("W-2"));
    assert_eq!(read.tax_year, Some(2025));
    // The file's own name for the employer agrees with the reading, and is cleaner.
    assert_eq!(read.issuer.as_deref(), Some("Example Global"));
    let field = |key: &str| read.fields.iter().find(|field| field.key == key).unwrap();
    // The copies disagree on box 1, so it waits for the user; box 2 agrees.
    assert!(field("box1").uncertain);
    assert_eq!(field("box2").amount, Some(1_433.04));
    assert!(!field("box2").uncertain);
    assert_eq!(field("box4").amount, Some(887.6));
    assert_eq!(field("box12:D").amount, Some(1_298.06));
    assert_eq!(field("state").text.as_deref(), Some("CA"));
    // 6.2% of 14,316.06 is 887.60.
    assert!(read.checks.iter().all(|check| check.passed));
}

/// A pay statement laid out as an earnings statement prints it, with a side panel.
fn payslip() -> Reading {
    vision(vec![
        line("EARNINGS STATEMENT", 0.5, 0.02, 0.3),
        line("Period Beginning: 05/31/2026", 0.55, 0.06, 0.3),
        line("Period Ending: 06/13/2026", 0.55, 0.08, 0.3),
        line("Check Date: 06/18/2026", 0.55, 0.1, 0.3),
        line(
            "Earnings Rate Hours/Units Amount Year-To-Date",
            0.05,
            0.2,
            0.45,
        ),
        line("Payment Method Amount", 0.6, 0.21, 0.3),
        line("Salary Exempt 80.00 4,807.69 19,230.76", 0.05, 0.23, 0.45),
        line("Example AAA 8,812.02", 0.6, 0.24, 0.3),
        line("Gross Pay 4,807.69 19,230.76", 0.05, 0.26, 0.45),
        line("Tax Deductions: Federal", 0.05, 0.29, 0.2),
        line("Withholding Tax 665.19- 2,795.49-", 0.05, 0.31, 0.45),
        line("EE Social Security Tax 298.08- 1,192.31-", 0.05, 0.33, 0.45),
        line("EE Medicare Tax 69.72- 278.85-", 0.05, 0.35, 0.45),
        line("Tax Deductions: California", 0.05, 0.37, 0.2),
        line("Withholding Tax 294.11- 1,201.03-", 0.05, 0.39, 0.45),
        line("EE Voluntary Disabilit 62.50- 250.00-", 0.05, 0.41, 0.45),
        line("Additional Deductions", 0.05, 0.43, 0.2),
        line("*401k EE pretax 240.38- 721.14-", 0.05, 0.45, 0.45),
        line("ESPP Contribution 240.38- 721.14-", 0.05, 0.47, 0.45),
        line(
            "Stale Dated Rep/Cash Fixe 5,874.69 8,812.04",
            0.05,
            0.49,
            0.45,
        ),
        line("Total Net Pay 8,812.02 20,882.84", 0.05, 0.52, 0.45),
        line("Example Services Group Inc.", 0.1, 0.6, 0.3),
        line(
            "Deposited to the account of: Payment Type Account No. Amount",
            0.1,
            0.7,
            0.7,
        ),
        line(
            "Sample Person Example AAA/Ca XXXXX8549 999999992 USD 8,812.02",
            0.1,
            0.72,
            0.7,
        ),
    ])
}

/// Two copies of a W-2 that prints its state row's figures above their labels, as some
/// payroll systems do, with a local tax box left empty in the other layout.
#[test]
fn figures_above_their_labels_and_box14_items_are_read_and_each_figure_has_one_box() {
    let mut lines = Vec::new();
    for top in [0.1, 0.5] {
        lines.extend([
            line(
                "Form W-2 Wage and Tax Statement 2025",
                0.05,
                top - 0.05,
                0.4,
            ),
            line("1 Wages, tips, other compensation", 0.3, top, 0.18),
            // One copy lost a digit; it doesn't dispute the others.
            line(
                if top < 0.3 { "1056.00" } else { "056.00" },
                0.32,
                top + 0.015,
                0.06,
            ),
            line("13 Statutory employee 14 Other", 0.05, top + 0.05, 0.2),
            line("FAMLI", 0.19, top + 0.065, 0.04),
            line("4.76", 0.25, top + 0.065, 0.03),
            line("12a Code", 0.4, top + 0.05, 0.08),
            line("CI 1.06", 0.41, top + 0.065, 0.06),
            // The state row: figures over their labels.
            line("1056.00", 0.3, top + 0.1, 0.06),
            line("20.00", 0.45, top + 0.1, 0.05),
            line("16 State wages, tips, etc.", 0.28, top + 0.115, 0.15),
            line("17 State income tax", 0.44, top + 0.115, 0.12),
            line("18 Local wages, tips, etc.", 0.05, top + 0.13, 0.15),
            line("19 Local income tax", 0.28, top + 0.13, 0.12),
        ]);
    }
    let read = understand(&vision(lines), "W-2 2025 Example.jpeg");
    let field = |key: &str| read.fields.iter().find(|field| field.key == key);
    assert_eq!(field("box1").and_then(|f| f.amount), Some(1_056.0));
    assert!(!field("box1").unwrap().uncertain);
    assert_eq!(field("box16").and_then(|f| f.amount), Some(1_056.0));
    assert_eq!(field("box17").and_then(|f| f.amount), Some(20.0));
    // Nothing under or over the local boxes: the state figures above them are taken.
    assert!(field("box18").is_none() && field("box19").is_none());
    assert_eq!(field("box14:FAMLI").and_then(|f| f.amount), Some(4.76));
    // The divider after a box 12 code reads as "I"; the code is still C.
    assert_eq!(field("box12:C").and_then(|f| f.amount), Some(1.06));
}

#[test]
fn a_pay_statement_reads_both_columns_and_balances_to_net_pay() {
    let read = understand(&payslip(), "Payslip_2026-06-18.pdf");
    assert_eq!(read.kind, Some(Kind::Payslip));
    assert_eq!(read.date.as_deref(), Some("2026-06-18"));
    assert_eq!(read.period_start.as_deref(), Some("2026-05-31"));
    assert_eq!(read.tax_year, Some(2026));
    assert_eq!(read.issuer.as_deref(), Some("Example Services Group Inc"));
    let field = |key: &str| read.fields.iter().find(|field| field.key == key).unwrap();
    assert_eq!(
        (field("gross").amount, field("gross").ytd),
        (Some(4_807.69), Some(19_230.76))
    );
    // The section decides whose withholding each line is.
    assert_eq!(field("federalWithholding").amount, Some(665.19));
    assert_eq!(field("stateWithholding").ytd, Some(1_201.03));
    assert_eq!(field("pretax401k").amount, Some(240.38));
    assert_eq!(field("espp").ytd, Some(721.14));
    assert_eq!(
        field("other:Stale Dated Rep/Cash Fixe").amount,
        Some(5_874.69)
    );
    // Hours are a column of their own, and the side panel's amount stays out of the lines.
    assert_eq!(field("earnings:Salary Exempt").amount, Some(4_807.69));
    assert!(read.fields.iter().all(|field| !field.label.contains("AAA")));
    assert_eq!(
        read.deposits,
        vec![PayDeposit {
            mask: Some("8549".into()),
            amount: 8_812.02
        }]
    );
    assert_eq!(read.checks.len(), 2);
    assert!(
        read.checks.iter().all(|check| check.passed),
        "{:?}",
        read.checks
    );
}

#[test]
fn a_bank_statement_becomes_rows_only_when_it_balances() {
    let text = |lines: &[&str]| Reading {
        pages: vec![Page {
            source: "text".into(),
            lines: lines
                .iter()
                .enumerate()
                .map(|(index, text)| line(text, 0.05, index as f64 * 0.02, 0.9))
                .collect(),
        }],
        encrypted: false,
    };
    let statement = [
        "Example Bank, N.A.",
        "December 23, 2025 through January 22, 2026",
        "Account Number: 000000123459967",
        "Beginning Balance $2,403.46",
        "Ending Balance $2,382.47",
        "TRANSACTION DETAIL",
        "Beginning Balance $2,403.46",
        "12/30 Venmo Payment 1041235821296 Web ID: 3264681992 -40.00 2,363.46",
        "01/08 Online Transfer From Chk ...6744 19.00 2,382.46",
        "Transaction#: 24590478817",
        "01/22 Interest Payment 0.01 2,382.47",
        "Ending Balance $2,382.47",
    ];
    let read = understand(&text(&statement), "20260122-statements-9967-.pdf");
    assert_eq!(read.kind, Some(Kind::Statement));
    assert_eq!(read.mask.as_deref(), Some("9967"));
    assert_eq!(read.period_end.as_deref(), Some("2026-01-22"));
    let (csv, rows) = read.statement_csv.clone().unwrap();
    assert_eq!(rows, 3);
    // December's row belongs to the year the period started in, and a wrapped line joins the
    // row above it.
    assert!(csv.contains("12/30/2025,\"Venmo Payment 1041235821296 Web ID: 3264681992\",-40.00"));
    assert!(csv.contains("Online Transfer From Chk ...6744 Transaction#: 24590478817"));
    assert!(crate::statements::parse(csv.as_bytes()).is_ok());

    // A label alone on its line names nothing below it, such as the mailing barcode; the file
    // name's last four stand.
    let mut mailed = statement;
    mailed[2] = "Account Number:\n0000002211 DRE 703 219 24320 NNNNNNNNNNN 1 000000000 15 0000";
    let read = understand(&text(&mailed), "20260122-statements-9967-.pdf");
    assert_eq!(read.mask.as_deref(), Some("9967"));

    let mut unbalanced = statement;
    unbalanced[4] = "Ending Balance $2,000.00";
    let read = understand(&text(&unbalanced), "statement.pdf");
    assert!(read.statement_csv.is_none());
    assert!(!read.checks[0].passed);
}

#[test]
fn a_statement_row_takes_only_the_line_printed_under_it() {
    let page = |lines: Vec<Line>| Page {
        source: "text".into(),
        lines,
    };
    let marker = |text: &str, y: f64| Line {
        h: 0.001,
        ..line(text, 0.05, y, 0.02)
    };
    let reading = Reading {
        pages: vec![
            page(vec![
                line("Example Bank, N.A.", 0.05, 0.02, 0.3),
                line("May 01, 2025 through May 30, 2025", 0.6, 0.04, 0.3),
                line("Beginning Balance $100.00", 0.05, 0.06, 0.6),
                line("Ending Balance $70.00", 0.05, 0.08, 0.6),
                line("TRANSACTION DETAIL", 0.05, 0.10, 0.3),
                line("Beginning Balance $100.00", 0.15, 0.12, 0.7),
                line(
                    "05/12 Venmo Payment 1000000001 -15.00 85.00",
                    0.05,
                    0.14,
                    0.9,
                ),
                // A statement's hidden marker, then the page's footer.
                marker("*end*transaction detail", 0.155),
                line("Page of", 0.75, 0.95, 0.05),
            ]),
            page(vec![
                line("May 01, 2025 through May 30, 2025", 0.6, 0.04, 0.3),
                line("Account Number:", 0.6, 0.06, 0.1),
                line("TRANSACTION DETAIL (continued)", 0.05, 0.08, 0.3),
                line(
                    "05/19 Card Purchase 05/18 Example Cafe -15.00 70.00",
                    0.05,
                    0.10,
                    0.9,
                ),
                line("Springfield IL Card 0000", 0.05, 0.12, 0.3),
                line("Ending Balance $70.00", 0.15, 0.14, 0.7),
                marker("*end*transaction detail", 0.155),
                marker("*start*disclosure message area", 0.20),
                line(
                    "IN CASE OF ERRORS OR QUESTIONS ABOUT YOUR TRANSFERS:",
                    0.05,
                    0.22,
                    0.5,
                ),
            ]),
        ],
        encrypted: false,
    };
    let read = understand(&reading, "statement.pdf");
    let (csv, rows) = read.statement_csv.unwrap();
    assert_eq!(rows, 2);
    assert!(csv.contains("05/12/2025,\"Venmo Payment 1000000001\",-15.00\n"));
    assert!(csv.contains(
        "05/19/2025,\"Card Purchase 05/18 Example Cafe Springfield IL Card 0000\",-15.00\n"
    ));
}

#[test]
fn a_new_reading_keeps_what_the_user_typed() {
    let previous = vec![Field {
        key: "box1".into(),
        label: "Wages".into(),
        amount: Some(100.0),
        edited: true,
        ..Field::default()
    }];
    let read = vec![
        Field {
            key: "box1".into(),
            amount: Some(90.0),
            ..Field::default()
        },
        Field {
            key: "box2".into(),
            amount: Some(10.0),
            ..Field::default()
        },
    ];
    let merged = merge_fields(&previous, read);
    assert_eq!(merged[0].amount, Some(100.0));
    assert_eq!(merged[1].amount, Some(10.0));
}

/// Reads a folder of real documents on this Mac and prints what Brief made of them. Run by
/// hand: `BRIEF_DOCUMENTS_DIR=… cargo test documents::tests::reads_a_folder -- --ignored
/// --nocapture`. Nothing it reads is kept.
#[test]
#[ignore]
fn reads_a_folder() {
    let Ok(directory) = std::env::var("BRIEF_DOCUMENTS_DIR") else {
        return;
    };
    let mut paths = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(directory)];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| {
                ["pdf", "jpeg", "jpg", "png"]
                    .contains(&extension.to_string_lossy().to_lowercase().as_str())
            }) {
                paths.push(path);
            }
        }
    }
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let quick = read::read_file(&path, false).unwrap();
        if std::env::var("BRIEF_DOCUMENTS_DUMP").is_ok_and(|dump| name.contains(&dump)) {
            for line in quick.pages.iter().flat_map(|page| &page.lines) {
                println!(
                    "  [{:.3} {:.3} {:.3} {:.3}] {}",
                    line.x, line.y, line.w, line.h, line.text
                );
            }
        }
        // The same path an import takes, so what prints is what Brief would save.
        let bytes = std::fs::read(&path).unwrap();
        let read = ingest(&path, &name, &bytes, &[], "now").unwrap();
        println!(
            "{name}: {:?} {:?} {:?} year {:?} date {:?} mask {:?} rows {:?}",
            read.kind,
            read.form,
            read.issuer,
            read.tax_year,
            read.date,
            read.mask,
            read.statement_rows
        );
        for field in &read.fields {
            println!(
                "    {} = {:?} / ytd {:?} {:?}{}",
                field.key,
                field.amount,
                field.ytd,
                field.text,
                if field.uncertain { " (uncertain)" } else { "" }
            );
        }
        for deposit in &read.deposits {
            println!("    deposit {:?} {}", deposit.mask, deposit.amount);
        }
        for check in &read.checks {
            println!("    check {}: {}", check.label, check.passed);
        }
        for row in read
            .statement_csv
            .iter()
            .flat_map(|csv| csv.lines().skip(1))
        {
            println!("    row {row}");
        }
    }
}
