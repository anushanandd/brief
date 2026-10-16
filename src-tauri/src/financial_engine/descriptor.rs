//! A readable title from a bank's raw descriptor, for transactions with no merchant name.
//!
//! Bank descriptors put the readable part first and trail it with references: confirmation and
//! reference numbers, ACH company IDs, wire routing, and dates. The title keeps the part before
//! the first such marker, then drops trailing IDs. Masked account numbers stay, since they tell
//! a transfer's accounts apart. The full descriptor stays in the description.
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
        title.to_owned()
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
                "TRANSFER MONEY TO BROKERAGE xxxxx1234",
            ),
            (
                "Online Transfer from CHK ...1234 transaction#: 20000000001",
                "Online Transfer from CHK ...1234",
            ),
            (
                "Online Transfer 20000000001 from Example Checking ######1234 transaction #: 20000000001 03/11",
                "Online Transfer from Example Checking ######1234",
            ),
            (
                "Online Realtime Transfer to Savings 1234 transaction#: 20000000001 reference#: 9000000001RX 05/04",
                "Online Realtime Transfer to Savings 1234",
            ),
            ("EXAMPLE CORP PAYROLL PPD ID: 9100000001", "EXAMPLE CORP PAYROLL"),
            (
                "Example Payee EXAMPLE A1B2C3D4E5F6 CCD ID: STRIPE0001",
                "Example Payee EXAMPLE",
            ),
            ("RTP From JANE DOE REFID: 20260502000000000P1ABC00", "RTP From JANE DOE"),
            (
                "ZELLE PAYMENT FROM JANE DOE CONFIRMATION# P2PABCDEFGHIJK",
                "ZELLE PAYMENT FROM JANE DOE",
            ),
            ("Zelle payment from JANE DOE BACabcdefg8h", "Zelle payment from JANE DOE"),
            ("Zelle payment from JANE DOE 23500000001", "Zelle payment from JANE DOE"),
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
            "ZELLE PAYMENT TO JANE DOE (XXX) XXX-1234",
        ] {
            assert_eq!(readable_title(name), name);
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
