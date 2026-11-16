//! Tests del mode fill de la Jam (`no-orbit --jam-agent`).

#![cfg(test)]

use super::agent::parse_args;

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parseja_els_arguments_basics() {
    let dir = tempfile::TempDir::new().unwrap();
    let brief = dir.path().join("brief.json");
    std::fs::write(&brief, "{}").unwrap();
    let repo = dir.path().join("proj");
    std::fs::create_dir_all(&repo).unwrap();

    let a = parse_args(&args(&[
        "no-orbit",
        "--jam-agent",
        "--brief",
        brief.to_string_lossy().as_ref(),
        "--repo",
        repo.to_string_lossy().as_ref(),
        "--engine",
        "godot4",
        "--duration",
        "600",
    ]))
    .unwrap();
    assert_eq!(a.brief_path, brief);
    assert_eq!(a.repo_path, repo);
    assert_eq!(a.engine, "godot4");
    assert_eq!(a.duration_secs, 600);
    assert!(a.build_cmd.is_none());
}

#[test]
fn falta_el_brief_dona_error() {
    let e = parse_args(&args(&["no-orbit", "--jam-agent", "--repo", "/tmp"])).unwrap_err();
    assert!(e.contains("--brief"), "msg: {e}");
}

#[test]
fn falta_el_repo_dona_error() {
    let e = parse_args(&args(&["no-orbit", "--jam-agent", "--brief", "/tmp/b.json"])).unwrap_err();
    assert!(e.contains("--repo"), "msg: {e}");
}

#[test]
fn un_brief_inexistent_dona_error() {
    let e = parse_args(&args(&[
        "no-orbit",
        "--jam-agent",
        "--brief",
        "/no/existeix/../de/brief.json",
        "--repo",
        "/tmp",
    ]))
    .unwrap_err();
    assert!(e.contains("brief"), "msg: {e}");
}

#[test]
fn el_build_cmd_opcional_torna_a_none_si_es_buit() {
    let dir = tempfile::TempDir::new().unwrap();
    let brief = dir.path().join("brief.json");
    std::fs::write(&brief, "{}").unwrap();

    let a = parse_args(&args(&[
        "no-orbit",
        "--jam-agent",
        "--brief",
        brief.to_string_lossy().as_ref(),
        "--repo",
        dir.path().to_string_lossy().as_ref(),
        "--build-cmd",
        "   ",
    ]))
    .unwrap();
    assert!(a.build_cmd.is_none());
}
