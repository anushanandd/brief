//! A readable title from a bank's raw descriptor, for transactions with no merchant name.
//!
//! Bank descriptors put the readable part first and trail it with references: confirmation and
//! reference numbers, ACH company IDs, wire routing, and dates. The title keeps the part before
//! the first such marker, then drops trailing IDs. Masked account numbers stay, since they tell
//! a transfer's accounts apart. The full descriptor stays in the description.
//!
//! A few kinds a bank spells out at length are read for who was paid: a card purchase
//! (`Card Purchase 07/24 SQ *EXAMPLE CAFE OAKLAND CA Card 1234` is Example Cafe), a direct
//! deposit, a Zelle payment, a wire, a book transfer, and an instant payment.
use regex::Regex;
use std::sync::LazyLock;

/// Where a descriptor's references begin; everything from here on is dropped.
static REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        \s*-?\s*
        (?:
            \bconfirm(?:ation)?\b
          | \breference(?:\s+number)?\s*\#?\s*:
          | \btransaction\s*\#
          | \btransaction\s+date\s*:
          | \b(?:ppd|ccd|web|tel)\s+id\s*:
          | \b(?:ref|refid|trn|imad)\s*:
          | \(period\b
        )",
    )
    .expect("descriptor reference pattern")
});

/// A date such as `05/04` or `10/05/2026`.
static DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{1,2}/\d{1,2}(?:/\d{2,4})?$").expect("descriptor date"));

/// A run of eight or more digits: a transaction, trace, or phone number, never a name.
fn long_number(token: &str) -> bool {
    token.len() >= 8 && token.bytes().all(|byte| byte.is_ascii_digit())
}

/// A generated ID such as `BACsuenrfv8d` or `A0L8L8P5L0N7`: letters and digits, no masking.
fn generated_id(token: &str) -> bool {
    token.len() >= 8
        && token.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && token.bytes().any(|byte| byte.is_ascii_digit())
        && token.bytes().any(|byte| byte.is_ascii_alphabetic())
        && !token.to_ascii_lowercase().starts_with("xx")
}

/// The descriptors a bank spells out at length, each with the part that names who was paid.
static SPELLED_OUT: LazyLock<[(Regex, &str); 6]> = LazyLock::new(|| {
    let pattern = |text: &str| Regex::new(text).expect("descriptor pattern");
    [
        (
            pattern(
                r"(?i)^card purchase(?: with pin)? \d{1,2}/\d{1,2} (?:\w{2,4} ?\* ?)?(.+?)(?: card ?\d{4})?$",
            ),
            "",
        ),
        (pattern(r"(?i)^direct dep(?:osit)? funds recvd (.+)$"), ""),
        (
            pattern(r"(?i)^zelle payment (?:to) (.+?)(?: \(xxx\).*)?$"),
            "Zelle to ",
        ),
        (
            pattern(r"(?i)^zelle payment (?:from) (.+?)(?: \(xxx\).*)?$"),
            "Zelle from ",
        ),
        (
            pattern(r"(?i)^online (?:domestic|international) wire transfer\b.*?\ba/c: ?(.+)$"),
            "Wire to ",
        ),
        (
            pattern(r"(?i)^book transfer credit b/o: ?(.+)$"),
            "Transfer from ",
        ),
    ]
});

/// A transfer between accounts (`Online Realtime Transfer to`, `TRANSFER MONEY FROM`), and the
/// last four digits of a masked account number (`XXXXX1234`, `...1234`).
static TRANSFER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:online (?:realtime )?transfer|transfer money) (to|from) (.+)$")
        .expect("transfer pattern")
});
static MASKED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:[x#]{3,}|\.{3})(\d{4})\b").expect("masked pattern"));

/// An instant payment names its sender after `From:Bnf-`, past the sending bank.
static INSTANT_PAYMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^real time payment credit recd from .*?\bfrom: ?bnf-(.+?)(?: US)?$")
        .expect("instant payment pattern")
});

/// A postal code and country closing an address (`02111 US`, `89101-4122 US`).
static POSTAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r" \d{5}(?:-\d{4})?(?: [A-Z]{2})?$").expect("postal pattern"));

/// The words before the place `text` ends with ("EXAMPLE CAFE OAKLAND CA" is EXAMPLE CAFE),
/// as the bank printed them; the whole text when it ends with no place.
fn before_place(text: &str) -> String {
    let text = POSTAL.replace(text, "");
    let words = crate::places::descriptor_merchant(&text)
        .split(' ')
        .filter(|word| !word.is_empty())
        .count();
    let tokens = text.split(' ').collect::<Vec<_>>();
    if words == 0 || words >= tokens.len() {
        return text.into_owned();
    }
    tokens[..words].join(" ")
}

/// Who a spelled-out descriptor names, when it is one.
fn spelled_out(original: &str) -> Option<String> {
    if let Some(sender) = INSTANT_PAYMENT.captures(original) {
        return Some(format!("Payment from {}", sender[1].trim()));
    }
    if let Some(transfer) = TRANSFER.captures(original) {
        let account = MASKED.replace_all(&transfer[2], "••$1");
        return Some(format!(
            "Transfer {} {account}",
            transfer[1].to_ascii_lowercase()
        ));
    }
    let (captures, lead) = SPELLED_OUT
        .iter()
        .find_map(|(pattern, lead)| Some((pattern.captures(original)?, *lead)))?;
    let mut name = before_place(captures[1].trim());
    // A payroll deposit often repeats its sender's first word at the end (`EXAMPLE CO EXAMPLE`).
    let words = name.split(' ').collect::<Vec<_>>();
    if words.len() > 1 && words[0].eq_ignore_ascii_case(words[words.len() - 1]) {
        name = words[..words.len() - 1].join(" ");
    }
    (name.chars().filter(|c| c.is_alphabetic()).count() >= 3).then(|| format!("{lead}{name}"))
}

pub(super) fn readable_title(descriptor: &str) -> String {
    let original = descriptor.split_whitespace().collect::<Vec<_>>().join(" ");
    let head = REFERENCE
        .find(&original)
        .map_or(original.as_str(), |found| &original[..found.start()]);
    let mut tokens = head
        .split(' ')
        .filter(|token| !long_number(token))
        .collect::<Vec<_>>();
    while let Some(last) = tokens.last() {
        let bare = last.trim_end_matches([':', '-', '#', ',']);
        if bare.is_empty() || DATE.is_match(bare) || generated_id(bare) {
            tokens.pop();
        } else {
            break;
        }
    }
    let title = tokens.join(" ");
    let title = title.trim_end_matches([' ', ':', '-', '#', ',']);
    // A descriptor that is all reference keeps its full text rather than vanish.
    if title.chars().filter(|c| c.is_alphabetic()).count() < 3 {
        original
    } else {
        spelled_out(title).unwrap_or_else(|| title.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::readable_title;

    #[test]
    fn keeps_the_readable_part_before_references() {
        for (descriptor, title) in [
            (
                "DEPOSIT FUNDS RECEIVED Confirmation - #T100001 From 5550001111",
                "DEPOSIT FUNDS RECEIVED",
            ),
            (
                "ATM Withdrawal EXAMPLE BANK MAIN ST SPRINGFIELD QC Transaction Date : 10/05/2026",
                "ATM Withdrawal EXAMPLE BANK MAIN ST SPRINGFIELD QC",
            ),
            (
                "TRANSFER MONEY TO BROKERAGE xxxxx1234 Reference Number: T100001",
                "Transfer to BROKERAGE ••1234",
            ),
            (
                "Online Transfer from CHK ...1234 transaction#: 20000000001",
                "Transfer from CHK ••1234",
            ),
            (
                "Online Transfer 20000000001 from Example Checking ######1234 transaction #: 20000000001 03/11",
                "Transfer from Example Checking ••1234",
            ),
            (
                "Online Realtime Transfer to Savings 1234 transaction#: 20000000001 reference#: 9000000001RX 05/04",
                "Transfer to Savings 1234",
            ),
            ("EXAMPLE CORP PAYROLL PPD ID: 9100000001", "EXAMPLE CORP PAYROLL"),
            (
                "Example Payee EXAMPLE A1B2C3D4E5F6 CCD ID: STRIPE0001",
                "Example Payee EXAMPLE",
            ),
            ("RTP From JANE DOE REFID: 20260502000000000P1ABC00", "RTP From JANE DOE"),
            (
                "ZELLE PAYMENT FROM JANE DOE CONFIRMATION# P2PABCDEFGHIJK",
                "Zelle from JANE DOE",
            ),
            ("Zelle payment from JANE DOE BACabcdefg8h", "Zelle from JANE DOE"),
            ("Zelle payment from JANE DOE 23500000001", "Zelle from JANE DOE"),
            ("FUNDS RECEIVED CONFIRM#3200000000001", "FUNDS RECEIVED"),
            (
                "EXAMPLE PRIVATE BANK NA (Period 07/02-07/27)",
                "EXAMPLE PRIVATE BANK NA",
            ),
        ] {
            assert_eq!(readable_title(descriptor), title, "{descriptor}");
        }
    }

    #[test]
    fn leaves_ordinary_names_alone() {
        for name in [
            "Example Coffee",
            "AUTOPAY PAYMENT - THANK YOU",
            "REMOTE ONLINE DEPOSIT # 1",
            "Account Fee Adj EXAMPLE FEE REIMBURSEMENT",
            "7ELEVEN 1234",
        ] {
            assert_eq!(readable_title(name), name);
        }
    }

    #[test]
    fn names_who_a_spelled_out_descriptor_paid() {
        for (descriptor, title) in [
            (
                "Card Purchase 07/24 Sq *Example Cafe Oakland CA Card 1234",
                "Example Cafe",
            ),
            (
                "Card Purchase With Pin 07/27 Example Food Svcs Arlington VA Card 1234",
                "Example Food Svcs",
            ),
            // A town the gazetteer doesn't know stays; only the state goes.
            (
                "Card Purchase 07/27 Example Food Svcs Glen Jean WV Card 1234",
                "Example Food Svcs Glen Jean",
            ),
            (
                "DIRECT DEP FUNDS RECVD Example Labs Inc EXAMPLE",
                "Example Labs Inc",
            ),
            (
                "ZELLE PAYMENT TO JANE DOE (XXX) XXX-1234",
                "Zelle to JANE DOE",
            ),
            ("Zelle Payment From Jane Doe", "Zelle from Jane Doe"),
            (
                "ONLINE DOMESTIC WIRE TRANSFER VIA: EXAMPLE BANK NYC/000000001 A/C: EXAMPLE PAY CORP BOSTON MA 02111 US",
                "Wire to EXAMPLE PAY CORP",
            ),
            (
                "BOOK TRANSFER CREDIT B/O: EXAMPLE GLOBAL, INC. LAS VEGAS NV 89101-4122 US",
                "Transfer from EXAMPLE GLOBAL, INC.",
            ),
            (
                "Real Time Payment Credit Recd From Aba/Contr Bnk-000000001 From:Bnf-Example Pay US",
                "Payment from Example Pay",
            ),
        ] {
            assert_eq!(readable_title(descriptor), title, "{descriptor}");
        }
    }

    #[test]
    fn keeps_a_descriptor_that_is_all_reference() {
        assert_eq!(
            readable_title("Confirmation #T100001"),
            "Confirmation #T100001"
        );
        assert_eq!(readable_title("  Example   Coffee "), "Example Coffee");
    }
}
