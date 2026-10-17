//! `validate arch`: the real dependencies between workspace crates (from
//! `cargo metadata`) must be allowed by `arch.ron`.
//!
//! An edge `A -> B` in `arch.ron` also allows `A` to depend on everything `B`
//! may reach (indirect dependencies are allowed, as in the crates diagram).

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

#[derive(Deserialize)]
pub struct Arch {
    /// Crate name -> crates it may depend on directly.
    pub crates: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(Deserialize)]
pub struct Package {
    pub name: String,
    #[serde(default)]
    id: String,
    pub dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
pub struct Dependency {
    pub name: String,
    /// Set only for path dependencies (workspace crates).
    #[serde(default)]
    pub path: Option<String>,
}

pub fn run(root: &Path) -> Result<(), String> {
    let arch_path = root.join("arch.ron");
    let text =
        std::fs::read_to_string(&arch_path).map_err(|e| format!("error: {}: {e}", arch_path.display()))?;
    let arch: Arch = ron::from_str(&text).map_err(|e| format!("error: arch.ron: {e}"))?;

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps", "--offline"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("error: cannot run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "error: cargo metadata failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let meta: Metadata =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("error: cargo metadata output: {e}"))?;
    let members: Vec<Package> = meta
        .packages
        .into_iter()
        .filter(|p| meta.workspace_members.contains(&p.id))
        .collect();

    let problems = check(&arch, &members);
    for p in &problems {
        println!("{p}");
    }
    let errors = problems.iter().filter(|p| p.starts_with("error")).count();
    if errors > 0 {
        return Err(format!("arch has {errors} error(s)"));
    }
    println!(
        "arch ok: {} crates, every dependency allowed by arch.ron",
        members.len()
    );
    Ok(())
}

/// Pure check, one line per problem (`error: ...` or `warning: ...`).
pub fn check(arch: &Arch, members: &[Package]) -> Vec<String> {
    let mut problems = Vec::new();

    for (name, deps) in &arch.crates {
        for d in deps {
            if !arch.crates.contains_key(d) {
                problems.push(format!("error: arch.ron: `{name}` lists unknown crate `{d}`"));
            }
        }
    }
    if let Some(cycle) = find_cycle(arch) {
        problems.push(format!("error: arch.ron: cycle {}", cycle.join(" -> ")));
        return problems;
    }

    for p in members {
        if !arch.crates.contains_key(&p.name) {
            problems.push(format!(
                "error: crate `{}` is not in arch.ron (add it to the diagram and arch.ron first)",
                p.name
            ));
            continue;
        }
        let reach = reachable(arch, &p.name);
        let mut seen = Vec::new();
        for d in p.dependencies.iter().filter(|d| d.path.is_some()) {
            if seen.contains(&d.name) {
                continue;
            }
            seen.push(d.name.clone());
            if !reach.contains(&d.name) {
                problems.push(format!(
                    "error: `{}` depends on `{}`, not allowed by arch.ron",
                    p.name, d.name
                ));
            }
        }
    }
    for name in arch.crates.keys() {
        if !members.iter().any(|p| p.name == *name) {
            problems.push(format!(
                "warning: arch.ron lists `{name}`, which is not in the workspace"
            ));
        }
    }
    problems
}

/// Every crate `from` may depend on, directly or not.
fn reachable(arch: &Arch, from: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut stack: Vec<&str> = arch
        .crates
        .get(from)
        .map(|d| d.iter().map(String::as_str).collect())
        .unwrap_or_default();
    while let Some(n) = stack.pop() {
        if out.iter().any(|o| o == n) {
            continue;
        }
        out.push(n.to_owned());
        if let Some(deps) = arch.crates.get(n) {
            stack.extend(deps.iter().map(String::as_str));
        }
    }
    out
}

fn find_cycle(arch: &Arch) -> Option<Vec<String>> {
    fn visit<'a>(
        arch: &'a Arch,
        n: &'a str,
        path: &mut Vec<&'a str>,
        done: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        if let Some(i) = path.iter().position(|p| *p == n) {
            let mut cycle: Vec<String> = path.get(i..)?.iter().map(|s| (*s).to_owned()).collect();
            cycle.push(n.to_owned());
            return Some(cycle);
        }
        if done.contains(&n) {
            return None;
        }
        path.push(n);
        for d in arch.crates.get(n).into_iter().flatten() {
            if let Some(c) = visit(arch, d, path, done) {
                return Some(c);
            }
        }
        path.pop();
        done.push(n);
        None
    }
    let mut done = Vec::new();
    for n in arch.crates.keys() {
        if let Some(c) = visit(arch, n, &mut Vec::new(), &mut done) {
            return Some(c);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arch(text: &str) -> Arch {
        ron::from_str(text).unwrap()
    }

    fn pkg(name: &str, deps: &[&str]) -> Package {
        Package {
            name: name.into(),
            id: String::new(),
            dependencies: deps
                .iter()
                .map(|d| Dependency {
                    name: (*d).into(),
                    path: Some(format!("/x/{d}")),
                })
                .chain(std::iter::once(Dependency {
                    name: "serde".into(),
                    path: None,
                }))
                .collect(),
        }
    }

    const ARCH: &str =
        r#"(crates: { "base": [], "defs": ["base"], "combat": ["defs"], "client": ["base"] })"#;

    #[test]
    fn indirect_edges_allowed() {
        let a = arch(ARCH);
        let members = [
            pkg("base", &[]),
            pkg("defs", &["base"]),
            pkg("combat", &["defs", "base"]),
            pkg("client", &["base"]),
        ];
        assert!(check(&a, &members).is_empty());
    }

    #[test]
    fn forbidden_edge_and_unknown_crate() {
        let a = arch(ARCH);
        let members = [
            pkg("base", &[]),
            pkg("defs", &["base"]),
            pkg("combat", &["defs"]),
            pkg("client", &["defs"]),
            pkg("rogue", &[]),
        ];
        let p = check(&a, &members);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p[0].contains("`client` depends on `defs`"));
        assert!(p[1].contains("`rogue` is not in arch.ron"));
    }

    #[test]
    fn cycle_detected() {
        let a = arch(r#"(crates: { "a": ["b"], "b": ["c"], "c": ["a"] })"#);
        let p = check(&a, &[]);
        assert_eq!(p.len(), 1);
        assert!(p[0].contains("cycle a -> b -> c -> a"));
    }
}
