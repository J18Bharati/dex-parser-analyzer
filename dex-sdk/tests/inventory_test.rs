// Modified by Zero (2026-11-01): new test file (fork M2). See NOTICE.
use dexpa_core::multidex::read_dex_buffers_from_apk;
use dexpa_core::parse_dex;
use dexpa_sdk::inventory::{
    Inventory, PREFIX_DEPTHS, descriptor_to_name, inventory, inventory_from_buffers,
};

const DATA: &str = "../dex-core/tests/data";

fn apk() -> Vec<Vec<u8>> {
    read_dex_buffers_from_apk(format!("{DATA}/multidex.apk")).expect("multidex.apk")
}

fn check_invariants(inv: &Inventory, buffers: &[Vec<u8>]) {
    let headers: Vec<usize> = buffers
        .iter()
        .map(|b| parse_dex(b).unwrap().header().class_defs_size as usize)
        .collect();
    assert_eq!(inv.dexes.len(), buffers.len());
    for (d, h) in inv.dexes.iter().zip(&headers) {
        assert_eq!(
            d.classes, *h,
            "dex {} class count equals its header's class_defs_size",
            d.dex_index
        );
    }
    let total: usize = headers.iter().sum();
    assert!(total > 0);
    assert_eq!(inv.classes.len() + inv.unreadable_classes, total);
    let methods: usize = inv.dexes.iter().map(|d| d.methods).sum();
    assert_eq!(
        inv.classes.iter().map(|c| c.methods).sum::<usize>(),
        methods
    );
    for depth in PREFIX_DEPTHS {
        let counted: usize = inv
            .prefixes
            .iter()
            .filter(|p| p.depth == depth)
            .map(|p| p.classes)
            .sum();
        let too_shallow = inv
            .classes
            .iter()
            .filter(|c| c.name.split('.').count() - 1 < depth)
            .count();
        assert_eq!(
            counted + too_shallow,
            inv.classes.len(),
            "depth {depth}: every class counted once or too shallow"
        );
    }
    let mut sorted = inv.prefixes.clone();
    sorted.sort_by(|a, b| (a.depth, &a.prefix).cmp(&(b.depth, &b.prefix)));
    assert_eq!(sorted, inv.prefixes, "deterministic order");
}

#[test]
fn descriptors_become_dotted_names() {
    assert_eq!(
        descriptor_to_name("Lcom/foo/Bar;").as_deref(),
        Some("com.foo.Bar")
    );
    assert_eq!(
        descriptor_to_name("Lcom/foo/Bar$Inner;").as_deref(),
        Some("com.foo.Bar$Inner")
    );
    assert_eq!(descriptor_to_name("LTop;").as_deref(), Some("Top"));
    assert_eq!(
        descriptor_to_name("[Lcom/foo/Bar;").as_deref(),
        Some("com.foo.Bar")
    );
    assert_eq!(descriptor_to_name("I"), None, "primitive");
    assert_eq!(descriptor_to_name("Lcom/foo/Bar"), None, "unterminated");
    assert_eq!(descriptor_to_name("L;"), None, "empty");
}

#[test]
fn multidex_apk_counts_add_up() {
    let buffers = apk();
    assert!(buffers.len() >= 2);
    check_invariants(&inventory_from_buffers(&buffers).unwrap(), &buffers);
}

#[test]
fn every_single_dex_fixture_counts_add_up() {
    for entry in std::fs::read_dir(DATA).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "dex") {
            let buffers = vec![std::fs::read(&path).unwrap()];
            check_invariants(&inventory_from_buffers(&buffers).unwrap(), &buffers);
        }
    }
}

#[test]
fn referenced_types_are_not_defined_classes() {
    let bytes = std::fs::read(format!("{DATA}/classes.dex")).unwrap();
    let dex = parse_dex(&bytes).unwrap();
    let referenced = (0..dex.type_count() as u32)
        .filter_map(|i| dex.type_descriptor(dexpa_core::format::TypeIdx(i)))
        .any(|t| t == "Ljava/lang/Object;");
    assert!(referenced, "the fixture references java.lang.Object");
    let inv = inventory(std::slice::from_ref(&dex));
    assert!(!inv.classes.is_empty());
    assert!(
        inv.classes.iter().all(|c| c.name != "java.lang.Object"),
        "referenced, never defined"
    );
    assert!(inv.prefixes.iter().all(|p| p.prefix != "java.lang"));
}

#[test]
fn same_class_in_two_dexes_is_listed_twice() {
    let a = std::fs::read(format!("{DATA}/Test.dex")).unwrap();
    let inv = inventory_from_buffers(&[a.clone(), a]).unwrap();
    let test: Vec<usize> = inv
        .classes
        .iter()
        .filter(|c| c.name == "Test")
        .map(|c| c.dex_index)
        .collect();
    assert_eq!(test, vec![0, 1]);
}

/// Golden summary of multidex.apk. Regenerate with `UPDATE_GOLDEN=1 cargo test -p dexpa-sdk`.
#[test]
fn multidex_apk_golden() {
    let inv = inventory_from_buffers(&apk()).unwrap();
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "dexes": inv.dexes,
        "class_count": inv.classes.len(),
        "unreadable_classes": inv.unreadable_classes,
        "prefixes": inv.prefixes,
    }))
    .unwrap()
        + "\n";
    let path = "tests/golden/multidex_inventory.json";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(path, &json).unwrap();
    }
    let expected = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        json, expected,
        "golden mismatch; regenerate with UPDATE_GOLDEN=1 if intended"
    );
}
