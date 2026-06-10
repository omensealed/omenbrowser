use std::collections::BTreeMap;

use omenbrowser_rs::browser::partials::{
    compose_markup_with_partials, extract_partial_specs, parse_partial_descriptor,
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
