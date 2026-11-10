//! HarfBuzz's subsetting test suite (`test/subset/`, ADR 0022): the
//! `*.tests` files parsed as `subset_test_suite.py` does, and the
//! arguments `run-tests.py` passes to `hb-subset`.

use std::path::{Path, PathBuf};

/// One test: a font, a profile, a subset and an instance.
#[derive(Clone, Debug)]
pub struct Case {
    /// The `.tests` file's stem.
    pub suite: String,
    pub font: PathBuf,
    pub profile: PathBuf,
    pub subset: String,
    pub instance: String,
    pub iup_optimize: bool,
}

/// The sections of a `.tests` file.
#[derive(Default, Debug)]
pub struct Suite {
    pub fonts: Vec<String>,
    pub profiles: Vec<String>,
    pub subsets: Vec<String>,
    pub instances: Vec<String>,
    pub options: Vec<String>,
    pub iup_options: Vec<String>,
}

/// `SubsetTestSuite._parse`.
pub fn parse_suite(text: &str) -> Result<Suite, String> {
    let mut s = Suite::default();
    let mut cur: Option<&str> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        match line {
            "FONTS:" | "PROFILES:" | "SUBSETS:" | "INSTANCES:" | "OPTIONS:" | "IUP_OPTIONS:" => {
                cur = Some(line)
            }
            _ => {
                let v = match cur.ok_or("Failed to parse test suite file.")? {
                    "FONTS:" => &mut s.fonts,
                    "PROFILES:" => &mut s.profiles,
                    "SUBSETS:" => &mut s.subsets,
                    "INSTANCES:" => &mut s.instances,
                    "OPTIONS:" => &mut s.options,
                    _ => &mut s.iup_options,
                };
                v.push(line.to_string());
            }
        }
    }
    Ok(s)
}

/// `SubsetTestSuite.tests`.
pub fn cases(data: &Path, suite_name: &str, s: &Suite) -> Vec<Case> {
    let mut out = Vec::new();
    for f in &s.fonts {
        for p in &s.profiles {
            for sub in &s.subsets {
                let instances: Vec<(String, bool)> = if s.instances.is_empty() {
                    vec![(String::new(), false)]
                } else if s.iup_options.is_empty() {
                    s.instances.iter().map(|i| (i.clone(), false)).collect()
                } else {
                    s.instances
                        .iter()
                        .flat_map(|i| s.iup_options.iter().map(move |o| (i.clone(), o == "Yes")))
                        .collect()
                };
                for (instance, iup) in instances {
                    out.push(Case {
                        suite: suite_name.to_string(),
                        font: data.join("fonts").join(f),
                        profile: data.join("profiles").join(p),
                        subset: sub.clone(),
                        instance,
                        iup_optimize: iup,
                    });
                }
            }
        }
    }
    out
}

impl Case {
    /// `Test.unicodes`.
    pub fn unicodes(&self) -> String {
        if self.subset == "*" {
            "*".to_string()
        } else if self.subset == "no-unicodes" {
            String::new()
        } else if self.subset.starts_with("U+") {
            self.subset.replace("U+", "")
        } else {
            self.subset
                .chars()
                .map(|c| format!("{:X}", c as u32))
                .collect::<Vec<_>>()
                .join(",")
        }
    }

    fn instance_name(&self) -> String {
        if self.instance.is_empty() {
            return String::new();
        }
        let axes: Vec<String> = self
            .instance
            .replace(':', "-")
            .split(',')
            .map(str::to_string)
            .collect();
        let mut s = format!(".{}", axes[..axes.len().min(6)].join(","));
        if self.iup_optimize {
            s.push_str(".iup_optimize");
        }
        s
    }

    /// `Test.get_font_name`: the expected file's name.
    pub fn font_name(&self) -> String {
        let base = self
            .font
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let ext = self
            .font
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let profile = self
            .profile
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let u = self.unicodes();
        let mid = match u.as_str() {
            "*" => "all".to_string(),
            "" => "no-unicodes".to_string(),
            _ => u,
        };
        format!("{base}.{profile}.{mid}{}{ext}", self.instance_name())
    }

    /// The test's identifier: `<suite>.tests/<expected file>`.
    pub fn id(&self) -> String {
        format!("{}.tests/{}", self.suite, self.font_name())
    }

    /// The expected output.
    pub fn expected(&self, data: &Path) -> PathBuf {
        data.join("expected")
            .join(&self.suite)
            .join(self.font_name())
    }

    /// The arguments of `run-tests.py` (without the output file).
    pub fn args(&self) -> Result<Vec<String>, String> {
        let mut a = vec![
            format!("--font-file={}", self.font.display()),
            format!("--unicodes={}", self.unicodes()),
            "--drop-tables+=DSIG,BASE".to_string(),
            "--drop-tables-=sbix".to_string(),
        ];
        let profile = std::fs::read_to_string(&self.profile).map_err(|e| e.to_string())?;
        a.extend(profile.lines().map(str::to_string));
        if !self.instance.is_empty() {
            a.push(format!("--instance={}", self.instance));
        }
        if self.iup_optimize {
            a.push("--optimize".to_string());
        }
        Ok(a)
    }
}

/// Every case of every `.tests` file, sorted by file name.
pub fn all_cases(data: &Path) -> Result<Vec<Case>, String> {
    let mut names: Vec<PathBuf> = std::fs::read_dir(data.join("tests"))
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "tests"))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for n in names {
        let text = std::fs::read_to_string(&n).map_err(|e| e.to_string())?;
        let suite = parse_suite(&text)?;
        let stem = n
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        out.extend(cases(data, &stem, &suite));
    }
    Ok(out)
}

/// The first difference of two fonts, by table: `tag@offset` of each
/// differing table, or the whole-file difference if the directories
/// differ.
pub fn diff(expected: &[u8], actual: &[u8]) -> String {
    use sofel_read::sfnt::Sfnt;
    let (Ok(e), Ok(a)) = (Sfnt::new(expected), Sfnt::new(actual)) else {
        return "not an sfnt".to_string();
    };
    let mut out = Vec::new();
    let et: Vec<_> = e.records().map(|r| r.tag).collect();
    let at: Vec<_> = a.records().map(|r| r.tag).collect();
    if et != at {
        out.push(format!(
            "tables {:?} vs {:?}",
            et.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
            at.iter().map(|t| t.to_string()).collect::<Vec<_>>()
        ));
    }
    for t in et {
        let (x, y) = (e.table(t).unwrap_or(&[]), a.table(t).unwrap_or(&[]));
        if x != y {
            let at = x
                .iter()
                .zip(y)
                .position(|(p, q)| p != q)
                .unwrap_or(x.len().min(y.len()));
            out.push(format!("{t}@{at} (len {} vs {})", x.len(), y.len()));
        }
    }
    if out.is_empty() && expected != actual {
        out.push("layout or padding differs".to_string());
    }
    out.join(", ")
}
