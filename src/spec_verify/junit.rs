//! JUnit XML test reports (LEP-008 D2, D4): the nextest runner's own report
//! and reports supplied with `--test-results` become the same per-test
//! records the cargo runner reads from libtest's result lines.

use std::path::Path;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::spec_core::{SpecError, SpecResult};

use super::test_batch::{LineOutcome, TestLine};

/// Parse a JUnit report. Each `<testcase>` becomes one [`TestLine`] whose
/// binary id is its `classname` (falling back to the enclosing
/// `<testsuite name>`), whose path is its `name`, and whose outcome is
/// failed for a `<failure>` or `<error>` child, ignored for `<skipped>`, and
/// passed otherwise.
pub fn parse_junit(xml: &str) -> Result<Vec<TestLine>, String> {
    let mut reader = Reader::from_str(xml);
    let mut lines = Vec::new();
    let mut suite: Option<String> = None;
    let mut case: Option<CaseBuilder> = None;
    let mut saw_root = false;
    // Inside a <failure>/<error>/<system-out>/<system-err> of the open case.
    let mut capture = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|err| format!("invalid XML at byte {}: {err}", reader.error_position()))?;
        match event {
            Event::Start(element) => match element.name().into_inner() {
                "testsuites" => saw_root = true,
                "testsuite" => {
                    saw_root = true;
                    suite = attribute(&element, "name")?;
                }
                "testcase" => case = Some(CaseBuilder::new(&element, suite.as_deref())?),
                name => {
                    if let Some(case) = case.as_mut() {
                        capture = case.child(name, &element)?;
                    }
                }
            },
            Event::Empty(element) => match element.name().into_inner() {
                "testsuites" | "testsuite" => saw_root = true,
                "testcase" => {
                    lines.push(CaseBuilder::new(&element, suite.as_deref())?.finish());
                }
                name => {
                    if let Some(case) = case.as_mut() {
                        case.child(name, &element)?;
                    }
                }
            },
            Event::End(element) => match element.name().into_inner() {
                "testcase" => {
                    if let Some(case) = case.take() {
                        lines.push(case.finish());
                    }
                    capture = false;
                }
                "testsuite" => suite = None,
                _ => capture = false,
            },
            Event::Text(text) if capture => {
                if let Some(case) = case.as_mut() {
                    case.detail.push_str(&text.xml10_content());
                }
            }
            Event::CData(text) if capture => {
                if let Some(case) = case.as_mut() {
                    case.detail.push_str(text.as_ref());
                }
            }
            Event::GeneralRef(reference) if capture => {
                if let Some(case) = case.as_mut() {
                    let name: &str = reference.as_ref();
                    let resolved = match reference.resolve_char_ref() {
                        Ok(Some(ch)) => ch.to_string(),
                        _ => quick_xml::escape::resolve_predefined_entity(name)
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("&{name};")),
                    };
                    case.detail.push_str(&resolved);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return Err("no <testsuites> or <testsuite> element".to_string());
    }
    Ok(lines)
}

/// Read and parse one report file; the error names the file.
pub fn load_junit(path: &Path) -> SpecResult<Vec<TestLine>> {
    let xml = std::fs::read_to_string(path).map_err(|err| {
        SpecError::Verification(format!(
            "cannot read test results {}: {err}",
            path.display()
        ))
    })?;
    parse_junit(&xml).map_err(|err| {
        SpecError::Verification(format!(
            "cannot parse test results {} as JUnit XML: {err}",
            path.display()
        ))
    })
}

struct CaseBuilder {
    binary_id: Option<String>,
    path: String,
    duration_ms: Option<u64>,
    outcome: LineOutcome,
    detail: String,
}

impl CaseBuilder {
    fn new(element: &BytesStart<'_>, suite: Option<&str>) -> Result<Self, String> {
        let path = attribute(element, "name")?
            .ok_or_else(|| "a <testcase> has no name attribute".to_string())?;
        let binary_id = attribute(element, "classname")?
            .filter(|classname| !classname.is_empty())
            .or_else(|| suite.map(str::to_string));
        let duration_ms = attribute(element, "time")?
            .and_then(|time| time.parse::<f64>().ok())
            .map(|seconds| (seconds * 1000.0).round() as u64);
        Ok(Self {
            binary_id,
            path,
            duration_ms,
            outcome: LineOutcome::Ok,
            detail: String::new(),
        })
    }

    /// Record a child element of the test case; returns whether its text is
    /// worth keeping as failure detail.
    fn child(&mut self, name: &str, element: &BytesStart<'_>) -> Result<bool, String> {
        match name {
            "failure" | "error" => {
                self.outcome = LineOutcome::Failed;
                if let Some(message) = attribute(element, "message")? {
                    self.detail.push_str(&message);
                    self.detail.push('\n');
                }
                Ok(true)
            }
            "skipped" => {
                if self.outcome != LineOutcome::Failed {
                    self.outcome = LineOutcome::Ignored;
                }
                Ok(false)
            }
            "system-out" | "system-err" => Ok(self.outcome == LineOutcome::Failed),
            _ => Ok(false),
        }
    }

    fn finish(self) -> TestLine {
        let detail = (self.outcome == LineOutcome::Failed && !self.detail.trim().is_empty())
            .then(|| self.detail.trim_end().to_string());
        TestLine {
            path: self.path,
            outcome: self.outcome,
            duration_ms: self.duration_ms,
            binary_id: self.binary_id,
            detail,
            report: None,
        }
    }
}

fn attribute(element: &BytesStart<'_>, key: &str) -> Result<Option<String>, String> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|err| format!("invalid attribute: {err}"))?;
        if attribute.key.into_inner() == key {
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|err| format!("invalid attribute value: {err}"))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The shape nextest 0.9.146 writes (probe crate, 2026-10-17).
    pub(crate) const NEXTEST_REPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="3" skipped="0" failures="1" errors="0">
    <testsuite name="probe-pkg" tests="1" skipped="0" errors="0" failures="0">
        <testcase name="tests::unit_ok" classname="probe-pkg" time="0.013"/>
    </testsuite>
    <testsuite name="probe-pkg::sys" tests="2" skipped="0" errors="0" failures="1">
        <testcase name="sys_ok" classname="probe-pkg::sys" time="0.017"/>
        <testcase name="sys_fail" classname="probe-pkg::sys" time="0.017">
            <failure message="thread &apos;sys_fail&apos; panicked at tests/sys.rs:2:25" type="test failure">thread &apos;sys_fail&apos; panicked at tests/sys.rs:2:25:
boom</failure>
            <system-out>running 1 test</system-out>
        </testcase>
    </testsuite>
</testsuites>
"#;

    #[test]
    fn test_parse_junit_reads_nextest_report() {
        let lines = parse_junit(NEXTEST_REPORT).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].binary_id.as_deref(), Some("probe-pkg"));
        assert_eq!(lines[0].path, "tests::unit_ok");
        assert_eq!(lines[0].outcome, LineOutcome::Ok);
        assert_eq!(lines[0].duration_ms, Some(13));
        assert_eq!(lines[2].binary_id.as_deref(), Some("probe-pkg::sys"));
        assert_eq!(lines[2].outcome, LineOutcome::Failed);
        let detail = lines[2].detail.as_deref().unwrap();
        assert!(detail.contains("thread 'sys_fail' panicked"), "{detail}");
        assert!(detail.contains("boom"), "{detail}");
        assert!(lines[0].detail.is_none());
    }

    #[test]
    fn test_parse_junit_error_and_skipped_children() {
        let xml = r#"<testsuite name="alpha::sys">
  <testcase name="errored"><error message="setup"/></testcase>
  <testcase name="skipped"><skipped/></testcase>
  <testcase name="plain"/>
</testsuite>"#;
        let lines = parse_junit(xml).unwrap();
        assert_eq!(
            lines.iter().map(|line| line.outcome).collect::<Vec<_>>(),
            [LineOutcome::Failed, LineOutcome::Ignored, LineOutcome::Ok]
        );
        // No classname: the suite name is the binary id.
        assert!(
            lines
                .iter()
                .all(|line| line.binary_id.as_deref() == Some("alpha::sys"))
        );
    }

    #[test]
    fn test_parse_junit_rejects_non_junit_input() {
        assert!(parse_junit("not xml").is_err());
        assert!(parse_junit("<html></html>").is_err());
        assert!(parse_junit("<testsuite><testcase></testsuite>").is_err());
    }
}
