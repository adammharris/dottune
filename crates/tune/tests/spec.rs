//! SPEC.md's conformance examples (§8), extracted mechanically: a ```tune
//! fence immediately followed by an ```events or ```error fence is an
//! example. Other tune fences are illustrations and are skipped.

struct Fence {
    lang: String,
    body: String,
    line: usize,
}

fn fences(markdown: &str) -> Vec<Fence> {
    let lines: Vec<&str> = markdown.split('\n').collect();
    let mut out = vec![];
    let mut i = 0;
    while i < lines.len() {
        if let Some(lang) = lines[i].trim_end().strip_prefix("```") {
            let start = i;
            let mut body = vec![];
            i += 1;
            while i < lines.len() && lines[i].trim_end() != "```" {
                body.push(lines[i]);
                i += 1;
            }
            out.push(Fence {
                lang: lang.to_string(),
                body: body.join("\n"),
                line: start + 1,
            });
        }
        i += 1;
    }
    out
}

#[test]
fn every_example_in_the_spec_holds() {
    let spec =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../SPEC.md")).unwrap();
    let fences = fences(&spec);
    let mut checked = 0;
    let mut failures = vec![];
    for pair in fences.windows(2) {
        let (tune, expect) = (&pair[0], &pair[1]);
        if tune.lang != "tune" || (expect.lang != "events" && expect.lang != "error") {
            continue;
        }
        checked += 1;
        let result = tune::compile(&tune.body);
        match (expect.lang.as_str(), result) {
            ("events", Ok(song)) => {
                let got = tune::format_song(&song);
                if got != expect.body.trim() {
                    failures.push(format!(
                        "SPEC.md:{}\nwant:\n{}\ngot:\n{got}",
                        tune.line,
                        expect.body.trim()
                    ));
                }
            }
            ("events", Err(e)) => failures.push(format!("SPEC.md:{}: {e}", tune.line)),
            (_, Ok(_)) => failures.push(format!(
                "SPEC.md:{}: compiled, want {}",
                tune.line,
                expect.body.trim()
            )),
            (_, Err(e)) => {
                let want: usize = expect
                    .body
                    .trim()
                    .trim_start_matches("line")
                    .trim()
                    .parse()
                    .unwrap();
                if e.first().line != want {
                    failures.push(format!(
                        "SPEC.md:{}: error on line {}, want {want}: {e}",
                        tune.line,
                        e.first().line
                    ));
                }
            }
        }
    }
    assert!(checked > 40, "only {checked} examples found");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn the_examples_compile() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let source = std::fs::read_to_string(&path).unwrap();
        if let Err(e) = tune::compile(&source) {
            panic!("{}: {e}", path.display());
        }
    }
}
