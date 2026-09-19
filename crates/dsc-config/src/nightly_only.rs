//! Signals the shipped defaults read that the latest stable DCS-BIOS lacks.
//!
//! The defaults are written against a DCS-BIOS nightly, and most users run the
//! stable release. Most of what the defaults read is in both, so there is
//! nothing to say about it. This is the short list of what is not: signals the
//! stable release does not have at all, or reports with a different range.
//!
//! It is built once per release of DCS Signal Converter, against whatever DCS-BIOS stable is
//! current at the time, and shipped as `data/nightly-only.json`. It is not a
//! history of DCS-BIOS: only the difference between the nightly the defaults
//! use and the stable a user is most likely to have. A user whose DCS-BIOS is
//! missing one of these is told which lamps need the nightly, and decides.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{read_json, Catalogue, Output, Profile, Result};

/// The shipped list: which stable and nightly it compares, and per module,
/// each signal that differs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NightlyOnly {
    pub stable: String,
    pub nightly: String,
    pub signals: BTreeMap<String, BTreeMap<String, Change>>,
}

/// How a signal in the nightly differs from the stable release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum Change {
    /// The stable release has no signal of that name in the module.
    Missing,
    /// Both have it, with a different highest value (a selector that gained a
    /// position) or, for text, a different length.
    Range { stable: Option<u32>, nightly: Option<u32> },
    /// Both have it, but one reports a number and the other text.
    Kind { stable: String, nightly: String },
}

impl NightlyOnly {
    /// Read the shipped list. A missing file is an empty list: nothing is
    /// known to be nightly-only, which is the right answer for a checkout that
    /// has not generated one.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        read_json(path)
    }

    /// How `signal` in `module` differs from stable, if it does.
    pub fn get(&self, module: &str, signal: &str) -> Option<&Change> {
        self.signals.get(module)?.get(signal)
    }

    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }

    /// Compare what `profiles` read, in the `nightly` catalogue, against the
    /// `stable` one. Returns the list, and every signal a profile names that
    /// the nightly itself does not have, which is a fault in the profile
    /// rather than a difference between releases.
    pub fn compare(
        profiles: &[Profile],
        nightly: &Catalogue,
        stable: &Catalogue,
    ) -> (Self, Vec<String>) {
        let mut list = NightlyOnly {
            stable: stable.bios_version().unwrap_or("unknown").to_string(),
            nightly: nightly.bios_version().unwrap_or("unknown").to_string(),
            signals: BTreeMap::new(),
        };
        let mut unknown = Vec::new();
        for profile in profiles {
            let Some(module) = nightly.module(&profile.module) else {
                unknown.push(format!("{}: module {} is not in the nightly", profile.name, profile.module));
                continue;
            };
            let stable_module = stable.module(&profile.module);
            for id in profile.signals_read() {
                let Some(now) = module.signal(id).and_then(|s| s.primary()) else {
                    unknown.push(format!("{}: {id} is not in {} in the nightly", profile.name, profile.module));
                    continue;
                };
                let then = stable_module.and_then(|m| m.signal(id)).and_then(|s| s.primary());
                if let Some(change) = difference(then, now) {
                    list.signals
                        .entry(profile.module.clone())
                        .or_default()
                        .insert(id.to_string(), change);
                }
            }
        }
        unknown.sort();
        unknown.dedup();
        (list, unknown)
    }
}

fn difference(stable: Option<&Output>, nightly: &Output) -> Option<Change> {
    let Some(stable) = stable else {
        return Some(Change::Missing);
    };
    if stable.r#type != nightly.r#type {
        return Some(Change::Kind {
            stable: stable.r#type.clone(),
            nightly: nightly.r#type.clone(),
        });
    }
    let reach = |o: &Output| {
        if o.r#type == "string" {
            o.max_length.map(u32::from)
        } else {
            o.max_value
        }
    };
    (reach(stable) != reach(nightly)).then(|| Change::Range {
        stable: reach(stable),
        nightly: reach(nightly),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Module;

    fn module(signals: serde_json::Value) -> Module {
        serde_json::from_value(serde_json::json!({
            "module": "Jet", "aircraft": ["Jet"], "signals": signals
        }))
        .unwrap()
    }

    fn signal(id: &str, kind: &str, max_value: u32, max_length: Option<u16>) -> serde_json::Value {
        serde_json::json!({"id": id, "outputs": [{
            "address": 0, "mask": null, "max_value": max_value,
            "max_length": max_length, "type": kind
        }]})
    }

    fn profile() -> Profile {
        serde_json::from_value(serde_json::json!({
            "name": "Jet", "aircraft": ["Jet"], "module": "Jet",
            "bindings": [
                {"device": "D", "led": "A", "conditions": [{"source": "SAME", "on_when": {"equals": 1}}]},
                {"device": "D", "led": "B", "any_of": [
                    {"conditions": [{"source": "NEW", "on_when": {"equals": 1}}]},
                    {"conditions": [{"source": "WIDER", "on_when": {"equals": 3}}]}
                ]},
                {"device": "D", "led": "C", "conditions": [{"source": "", "on_when": {"equals": 1}}]}
            ],
            "readouts": [{"device": "D", "display": "S", "cells": "0-5", "source": "TEXT"}]
        }))
        .unwrap()
    }

    #[test]
    fn only_what_differs_is_listed() {
        let nightly = Catalogue::from_modules(vec![module(serde_json::json!([
            signal("SAME", "integer", 1, None),
            signal("NEW", "integer", 1, None),
            signal("WIDER", "integer", 3, None),
            signal("TEXT", "string", 0, Some(8)),
            signal("UNUSED", "integer", 1, None),
        ]))]);
        let stable = Catalogue::from_modules(vec![module(serde_json::json!([
            signal("SAME", "integer", 1, None),
            signal("WIDER", "integer", 2, None),
            signal("TEXT", "string", 0, Some(6)),
        ]))]);
        let (list, unknown) = NightlyOnly::compare(&[profile()], &nightly, &stable);
        assert!(unknown.is_empty(), "{unknown:?}");
        assert_eq!(list.get("Jet", "NEW"), Some(&Change::Missing));
        assert_eq!(
            list.get("Jet", "WIDER"),
            Some(&Change::Range { stable: Some(2), nightly: Some(3) })
        );
        assert_eq!(
            list.get("Jet", "TEXT"),
            Some(&Change::Range { stable: Some(6), nightly: Some(8) })
        );
        // Unchanged, and unused, signals say nothing.
        assert_eq!(list.get("Jet", "SAME"), None);
        assert_eq!(list.get("Jet", "UNUSED"), None);
        assert_eq!(list.signals["Jet"].len(), 3);
    }

    #[test]
    fn a_signal_the_nightly_lacks_is_a_fault_not_a_difference() {
        let nightly = Catalogue::from_modules(vec![module(serde_json::json!([
            signal("SAME", "integer", 1, None),
        ]))]);
        let stable = Catalogue::from_modules(vec![module(serde_json::json!([]))]);
        let (list, unknown) = NightlyOnly::compare(&[profile()], &nightly, &stable);
        assert_eq!(unknown.len(), 3, "{unknown:?}");
        assert_eq!(list.get("Jet", "SAME"), Some(&Change::Missing));
    }

    #[test]
    fn the_list_reads_back_as_written() {
        let mut list = NightlyOnly { stable: "0.11.7".into(), nightly: "n".into(), ..Default::default() };
        list.signals.entry("F-14".into()).or_default().insert("RIO_CDNU_LINE1".into(), Change::Missing);
        let text = serde_json::to_string(&list).unwrap();
        assert!(text.contains(r#""change":"missing""#), "{text}");
        assert_eq!(serde_json::from_str::<NightlyOnly>(&text).unwrap(), list);
        assert!(NightlyOnly::load(Path::new("no/such/file.json")).unwrap().is_empty());
    }
}
