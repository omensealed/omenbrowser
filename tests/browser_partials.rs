use std::collections::BTreeMap;

use omenbrowser_rs::browser::partials::{
    compose_markup_with_partials, extract_partial_specs, parse_partial_descriptor,
    try_parse_partial_descriptor, PARTIAL_ID_MAX_BYTES, PARTIAL_SPECS_MAX_BYTES,
    PARTIAL_SPEC_MAX_ITEMS,
};
use omenbrowser_rs::micron::parser::{
    MICRON_LINK_FIELD_MAX_BYTES, MICRON_LINK_MAX_FIELDS, MICRON_LINK_TARGET_MAX_BYTES,
};

#[test]
fn parses_partial_descriptor_fields() {
    let parsed = parse_partial_descriptor("mock.node:/feed`2`message|pid=feed|loop=3}");

    assert_eq!(parsed.target, "mock.node:/feed");
    assert_eq!(parsed.refresh_secs, Some(2.0));
    assert_eq!(parsed.fields, vec!["message"]);
    assert_eq!(parsed.id.as_deref(), Some("feed"));
    assert_eq!(parsed.loop_count, Some(3));
}

#[test]
fn partial_descriptor_loop_uses_python_bounds() {
    let parsed = parse_partial_descriptor("mock.node:/feed`1`pid=feed|loop=-2}");

    assert_eq!(parsed.loop_count, Some(0));
}

#[test]
fn partial_descriptor_rejects_oversized_targets_fields_and_ids() {
    let too_many_fields = (0..=MICRON_LINK_MAX_FIELDS)
        .map(|index| format!("field{index}"))
        .collect::<Vec<_>>()
        .join("|");
    for descriptor in [
        format!(
            "{}`1`message}}",
            "t".repeat(MICRON_LINK_TARGET_MAX_BYTES + 1)
        ),
        format!(
            "mock.node:/feed`1`{}}}",
            "f".repeat(MICRON_LINK_FIELD_MAX_BYTES + 1)
        ),
        format!("mock.node:/feed`1`{too_many_fields}}}"),
        format!(
            "mock.node:/feed`1`pid={}}}",
            "p".repeat(PARTIAL_ID_MAX_BYTES + 1)
        ),
    ] {
        assert!(try_parse_partial_descriptor(&descriptor).is_none());
        assert!(parse_partial_descriptor(&descriptor).target.is_empty());
        assert!(extract_partial_specs(&format!("`{{{descriptor}")).is_empty());
    }
}

#[test]
fn partial_spec_collection_is_item_and_byte_bounded() {
    let item_markup = (0..PARTIAL_SPEC_MAX_ITEMS + 10)
        .map(|index| format!("`{{mock.node:/feed/{index}`1`pid=p{index}}}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        extract_partial_specs(&item_markup).len(),
        PARTIAL_SPEC_MAX_ITEMS
    );

    let target = "t".repeat(MICRON_LINK_TARGET_MAX_BYTES);
    let byte_markup = (0..PARTIAL_SPEC_MAX_ITEMS)
        .map(|index| format!("`{{{target}`1`pid=p{index}}}"))
        .collect::<Vec<_>>()
        .join("\n");
    let specs = extract_partial_specs(&byte_markup);
    assert!(!specs.is_empty());
    assert!(specs.len() < PARTIAL_SPEC_MAX_ITEMS);
    assert!(
        specs
            .iter()
            .map(|spec| spec.target.len() + spec.id.as_deref().map_or(0, str::len))
            .sum::<usize>()
            < PARTIAL_SPECS_MAX_BYTES
    );
}

#[test]
fn extracts_and_composes_partial_specs() {
    let markup = "before\n`{mock.node:/feed`2`pid=feed}\nafter";
    let specs = extract_partial_specs(markup);
    let mut contents = BTreeMap::new();
    contents.insert(specs[0].slot.clone(), "loaded".into());

    let composed = compose_markup_with_partials(markup, &specs, &contents);

    assert_eq!(specs.len(), 1);
    assert_eq!(composed, "before\nloaded\nafter");
}

#[test]
fn composed_partial_strips_micron_document_header() {
    let markup = "before\n`{mock.node:/feed`2`pid=feed}\nafter";
    let specs = extract_partial_specs(markup);
    let mut contents = BTreeMap::new();
    contents.insert(specs[0].slot.clone(), "#!c=0\nloaded".into());

    let composed = compose_markup_with_partials(markup, &specs, &contents);

    assert_eq!(composed, "before\nloaded\nafter");
}
