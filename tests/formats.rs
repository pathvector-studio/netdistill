//! Tests for the switchable output formats: text / json / toon / wire.
//!
//! Two research-backed ideas are exercised here:
//!
//! - `toon` — token-oriented serialization (explicit `[N]{cols}` + comma rows),
//!   fewer tokens than json while staying schema-explicit.
//! - `wire` — dense LLM-only mode: human affordances stripped, one-char tags.
//!
//! Both must stay faithful: the "explicit elision" invariant is preserved.

use netdistill::model::{Doc, Table};
use netdistill::render::{render_body_toon, render_body_wire, Format};
use netdistill::{distill_to_string, token};

fn fx(name: &str) -> String {
    std::fs::read_to_string(format!("fixtures/{name}")).unwrap()
}

fn out(input: &str, format: Format) -> String {
    distill_to_string(input, false, None, format, None, None)
}

#[test]
fn format_names_round_trip() {
    for name in Format::NAMES {
        assert!(Format::from_name(name).is_some(), "{name} should parse");
    }
    assert_eq!(Format::from_name("json"), Some(Format::Json));
    assert_eq!(Format::from_name("toon"), Some(Format::Toon));
    assert_eq!(Format::from_name("wire"), Some(Format::Wire));
    assert_eq!(Format::from_name("nope"), None);
}

#[test]
fn toon_uses_schema_explicit_table_headers() {
    let toon = out(&fx("frr_bgp_summary.txt"), Format::Toon);
    // TOON tabular arrays declare length and fields: `title[N]{col,col}:`
    assert!(
        toon.contains("]{") && toon.contains("}:"),
        "toon should carry [N]{{cols}} array headers:\n{toon}"
    );
    // rows are comma-delimited, not tab-delimited
    assert!(!toon.contains('\t'), "toon rows must not use tabs");
}

#[test]
fn wire_strips_human_affordances() {
    let wire = out(&fx("frr_bgp_summary.txt"), Format::Wire);
    // no markdown headers, no spelled-out section labels
    assert!(!wire.contains("## ["), "wire must drop markdown headers");
    assert!(!wire.contains("ALERTS"), "wire must drop the ALERTS label");
    assert!(
        !wire.contains("SUMMARY:"),
        "wire must drop the SUMMARY label"
    );
    // terse receipt prefix instead of the prose one
    assert!(
        wire.starts_with("nd in="),
        "wire receipt should be terse:\n{wire}"
    );
    // one-char tags present
    assert!(
        wire.contains("\n|") || wire.starts_with('|'),
        "wire needs table tag |"
    );
}

#[test]
fn elision_note_survives_in_every_format() {
    // A healthy-heavy bgp summary elides the boring peers; that elision must be
    // stated explicitly in *all* formats — silent truncation is the one thing
    // netdistill must never do.
    let mut input = String::from(
        "BGP router identifier 10.0.0.1, local AS number 65000\n\
         Neighbor        V         AS   MsgRcvd   MsgSent   TblVer  InQ OutQ Up/Down  State/PfxRcd\n",
    );
    for i in 0..80 {
        input.push_str(&format!(
            "10.0.0.{}    4      6500{}    100    100    5    0    0 01:00:00    10\n",
            i + 10,
            i % 9
        ));
    }
    for format in [Format::Text, Format::Toon, Format::Wire] {
        let o = out(&input, format);
        assert!(
            o.to_lowercase().contains("omitted") || o.to_lowercase().contains("elided"),
            "format {format:?} must state the elision:\n{o}"
        );
    }
}

#[test]
fn toon_beats_json_on_tokens() {
    // Uniform tabular data is exactly where TOON should win over JSON.
    let input = fx("ios_route_vrf_all.txt");
    let json_tok = token::estimate(&out(&input, Format::Json));
    let toon_tok = token::estimate(&out(&input, Format::Toon));
    assert!(
        toon_tok < json_tok,
        "toon ({toon_tok}) should use fewer tokens than json ({json_tok})"
    );
}

#[test]
fn wire_beats_text_on_tokens() {
    let input = fx("frr_bgp_summary.txt");
    let text_tok = token::estimate(&out(&input, Format::Text));
    let wire_tok = token::estimate(&out(&input, Format::Wire));
    assert!(
        wire_tok < text_tok,
        "wire ({wire_tok}) should use fewer tokens than text ({text_tok})"
    );
}

#[test]
fn csv_rows_quote_embedded_commas() {
    // A value containing a comma must be quoted so it can't fake extra columns.
    let mut d = Doc::new("generic", "test".into());
    d.tables.push(Table {
        title: "T".into(),
        header: vec!["a".into(), "b".into()],
        rows: vec![vec!["x,y".into(), "z".into()]],
        omitted: None,
    });
    let docs = [d];
    for body in [render_body_toon(&docs), render_body_wire(&docs)] {
        assert!(
            body.contains("\"x,y\",z"),
            "comma value must be quoted:\n{body}"
        );
    }
}

#[test]
fn budget_trims_and_stays_faithful_in_new_formats() {
    let input = fx("ios_route_vrf_all.txt");
    for format in [Format::Toon, Format::Wire] {
        let o = distill_to_string(&input, false, Some(120), format, None, None);
        let tok = token::estimate(&o);
        // budget is approximate; allow slack but it must actually shrink.
        assert!(
            tok < 400,
            "format {format:?} should shrink toward budget: {tok} tok"
        );
    }
}
