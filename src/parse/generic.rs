use crate::model::Doc;

/// Fallback for output with no dedicated parser: lossless-ish compaction.
/// Squeezes whitespace padding, collapses duplicate-line runs and blank runs.
pub fn parse(input: &str) -> Doc {
    let mut out: Vec<String> = Vec::new();
    let mut dup_runs = 0usize;
    let mut blank = false;
    let mut prev: Option<String> = None;
    let mut run = 0usize;

    for raw in input.lines() {
        let line = squeeze(raw);
        if line.trim().is_empty() {
            if let Some(p) = prev.take() {
                push_run(&mut out, p, run, &mut dup_runs);
                run = 0;
            }
            if !blank {
                out.push(String::new());
                blank = true;
            }
            continue;
        }
        blank = false;
        match &prev {
            Some(p) if *p == line => run += 1,
            _ => {
                if let Some(p) = prev.take() {
                    push_run(&mut out, p, run, &mut dup_runs);
                }
                prev = Some(line);
                run = 1;
            }
        }
    }
    if let Some(p) = prev.take() {
        push_run(&mut out, p, run, &mut dup_runs);
    }

    let mut doc = Doc::new(
        "generic",
        "no dedicated parser matched — compacted pass-through".into(),
    );
    doc.notes.push(
        "transformations: whitespace padding squeezed to tabs, consecutive duplicate lines collapsed as [xN], blank runs collapsed"
            .into(),
    );
    if dup_runs > 0 {
        doc.notes
            .push(format!("{dup_runs} duplicate-line run(s) collapsed"));
    }
    doc.body = Some(out.join("\n"));
    doc
}

fn push_run(out: &mut Vec<String>, line: String, run: usize, dup_runs: &mut usize) {
    if run > 1 {
        *dup_runs += 1;
        out.push(format!("{line}  [x{run}]"));
    } else {
        out.push(line);
    }
}

/// Collapse runs of 2+ spaces (column padding) into a single tab; cap leading
/// indent at one space. Fixed-width padding is pure token waste for an LLM.
fn squeeze(raw: &str) -> String {
    let t = raw.trim_end();
    let mut outs = String::new();
    let mut spaces = 0usize;
    let mut started = false;
    for ch in t.chars() {
        if ch == ' ' || ch == '\t' {
            spaces += 1;
        } else {
            if spaces > 0 {
                if !started || spaces == 1 {
                    outs.push(' ');
                } else {
                    outs.push('\t');
                }
                spaces = 0;
            }
            started = true;
            outs.push(ch);
        }
    }
    outs
}
