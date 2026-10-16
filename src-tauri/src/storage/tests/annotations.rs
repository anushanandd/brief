//! Tags, splits, and refund links.
use super::*;

#[test]
fn tags_splits_and_refund_links_are_validated_persisted_and_cleared() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let mut snapshot: Value =
        serde_json::from_str(include_str!("../../../../src/data/seed.json")).unwrap();
    let base = snapshot["transactions"][0].clone();
    let transaction = |id: &str, amount: f64| {
        let mut value = base.clone();
        value["id"] = id.into();
        value["amount"] = amount.into();
        value
    };
    snapshot["transactions"] = vec![transaction("buy", -100.0), transaction("refund", 40.0)].into();
    storage.data.snapshot = crate::finance_contract::decode(snapshot).unwrap();
    let split = |category: &str, amount: f64| Split {
        category: category.into(),
        amount,
    };
    let save = |storage: &mut Storage, id: &str, annotation: Annotation| {
        storage.save_annotations(BTreeMap::from([(id.to_owned(), annotation)]), false)
    };

    // Parts must share the sign and sum to the amount in cents.
    for parts in [
        vec![split("Groceries", -60.0), split("Gifts", -30.0)],
        vec![split("Groceries", -130.0), split("Gifts", 30.0)],
        vec![split("Groceries", -100.0)],
        vec![split(" ", -60.0), split("Gifts", -40.0)],
    ] {
        assert!(save(
            &mut storage,
            "buy",
            Annotation {
                splits: Some(parts),
                ..Default::default()
            }
        )
        .is_err());
    }
    assert!(save(
        &mut storage,
        "gone",
        Annotation {
            splits: Some(vec![split("A", -1.0), split("B", -1.0)]),
            ..Default::default()
        }
    )
    .is_err());
    // A refund links a credit to a purchase, never the reverse or itself.
    for (id, purchase) in [("buy", "refund"), ("refund", "refund"), ("refund", "gone")] {
        assert!(save(
            &mut storage,
            id,
            Annotation {
                refund_of: Some(purchase.into()),
                ..Default::default()
            }
        )
        .is_err());
    }
    // Marking a transaction yours is the default, so it leaves nothing to save.
    save(
        &mut storage,
        "buy",
        Annotation {
            other: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        storage.data.annotations.is_empty(),
        "rejected edits change nothing"
    );

    save(
        &mut storage,
        "buy",
        Annotation {
            tags: Some(vec![
                " Trip ".into(),
                "trip".into(),
                "".into(),
                "work".into(),
            ]),
            splits: Some(vec![split(" Groceries ", -60.1), split("Gifts", -39.9)]),
            other: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    save(
        &mut storage,
        "refund",
        Annotation {
            refund_of: Some("buy".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let saved = storage.database.load().unwrap().unwrap().annotations;
    assert_eq!(
        saved["buy"].tags,
        Some(vec!["Trip".to_owned(), "work".to_owned()])
    );
    assert_eq!(
        saved["buy"].splits,
        Some(vec![split("Groceries", -60.1), split("Gifts", -39.9)])
    );
    assert_eq!(saved["refund"].refund_of.as_deref(), Some("buy"));
    assert_eq!(saved["buy"].other, Some(true));
    let view = storage.snapshot().unwrap();
    assert_eq!(view["transactions"][0]["other"], true);
    assert_eq!(view["transactions"][0]["classification"]["kind"], "owed");
    // The linked refund is theirs too.
    assert_eq!(view["transactions"][1]["other"], true);
    assert_eq!(
        view["transactions"][0]["tags"],
        serde_json::json!(["Trip", "work"])
    );
    assert_eq!(view["transactions"][1]["refundOf"], "buy");

    // Empty values clear each field, and an empty annotation is removed.
    save(
        &mut storage,
        "buy",
        Annotation {
            tags: Some(vec![]),
            splits: Some(vec![]),
            other: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    save(
        &mut storage,
        "refund",
        Annotation {
            refund_of: Some(String::new()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(storage
        .database
        .load()
        .unwrap()
        .unwrap()
        .annotations
        .is_empty());
    fs::remove_dir_all(directory).unwrap();
}
