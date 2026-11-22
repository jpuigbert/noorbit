//! E2E «frase → fitxers → verificació verda» del nucli xat→codi (Fase 1e).
//!
//! Camí REAL complet, sense Tauri ni xarxa: la frase de l'usuari entra al
//! nivell determinista de la IA nativa (`ai::native::deterministic` →
//! `codegen::starter`), la resposta «@file:» es materialitza amb el MATEIX
//! bucle d'escriptura que usa el backend (`runtools::write_announced_files_at`,
//! compartit amb `write_announced_files`) dins un directori temporal, i els
//! fitxers resultants es verifiquen amb EINES REALS (`php -l` via
//! `lint_paths_blocking`). Si PHP no és instal·lat, la part de lint s'omet
//! suaument (la resta del camí es comprova igual).
//!
//! Cap camí autònom pot reportar èxit sense verificació: ací es garanteix
//! que les plantilles deterministes (tema WordPress, projecte Godot)
//! produeixen fitxers que PASSSEN la verificació de veritat.

use no_orbit_lib::agent::runtools;
use no_orbit_lib::ai::native;

/// Resposta determinista per a una frase (nivell 1 de la nativa: sense
/// cervell après ni lliçons — exactament el camí «frase → plantilla»).
fn resposta_determinista(frase: &str) -> String {
    native::deterministic(frase, &[], "")
        .unwrap_or_else(|| panic!("la nativa no ha donat resposta determinista per a «{frase}»"))
}

/// Els 5 fitxers canònics d'un tema WordPress mínim amb el nom donat.
fn fitxers_tema_wp(nom: &str) -> Vec<String> {
    ["style.css", "functions.php", "index.php", "header.php", "footer.php"]
        .iter()
        .map(|f| format!("wp-content/themes/{nom}/{f}"))
        .collect()
}

/// «crea un tema de wordpress anomenat Orbita» → 5 fitxers canònics escrits
/// al disc amb el camí real d'escriptura → `php -l` verd sobre els .php.
#[test]
fn frase_tema_wordpress_produeix_cinc_fitxers_i_php_l_verd() {
    let frase = "crea un tema de wordpress anomenat Orbita";
    let resp = resposta_determinista(frase);
    assert!(resp.contains("@file:"), "la resposta ha d'anunciar fitxers: {resp}");

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut written = runtools::write_announced_files_at(root, &resp);
    written.sort();
    let mut esperats = fitxers_tema_wp("orbita");
    esperats.sort();
    assert_eq!(written, esperats, "frase: «{frase}»");
    for rel in &written {
        assert!(root.join(rel).is_file(), "falta el fitxer escrit: {rel}");
    }
    // El header canònic del tema porta el nom demanat.
    let style = std::fs::read_to_string(root.join("wp-content/themes/orbita/style.css")).unwrap();
    assert!(style.contains("Theme Name: Orbita"), "style.css: {style}");
    // index.php ha de contenir el loop de WordPress (no un fitxer buit).
    let index = std::fs::read_to_string(root.join("wp-content/themes/orbita/index.php")).unwrap();
    assert!(index.contains("have_posts"), "index.php sense loop: {index}");

    // VERIFICACIÓ REAL: php -l sobre els fitxers escrits (el mateix motor
    // que el bucle de verificació del xat). Sense PHP instal·lat, s'omet.
    if runtools::php_binary().is_none() {
        eprintln!("php no disponible en aquesta màquina: verificació lint omesa");
        return;
    }
    let lint = runtools::lint_paths_blocking(root, &written);
    for r in &lint {
        if r.path.ends_with(".php") {
            assert!(r.ok && r.checked, "php -l ha de passar sobre {}: {:?}", r.path, r);
        }
    }
    let informe = runtools::verification_report(&lint, 0).expect("informe de verificació");
    assert!(informe.contains("cap error de sintaxi"), "informe: {informe}");
}

/// La frase EXACTA de la tasca («…anomenat X», nom d'1 lletra que el
/// extractor de noms descarta) també produeix un tema complet: el nom cau
/// al per defecte («tema») i els 5 fitxers segueixen sent canònics i vàlids.
#[test]
fn frase_tema_wordpress_amb_nom_duna_lletra_usa_el_nom_per_defecte() {
    let resp = resposta_determinista("crea un tema de wordpress anomenat X");
    let dir = tempfile::tempdir().unwrap();
    let mut written = runtools::write_announced_files_at(dir.path(), &resp);
    written.sort();
    let mut esperats = fitxers_tema_wp("tema");
    esperats.sort();
    assert_eq!(written, esperats);
    if runtools::php_binary().is_none() {
        eprintln!("php no disponible en aquesta màquina: verificació lint omesa");
        return;
    }
    let lint = runtools::lint_paths_blocking(dir.path(), &written);
    assert!(lint.iter().all(|r| r.ok), "lint: {lint:?}");
}

/// «fes un joc» → projecte Godot 4 REAL: project.godot + main.tscn + main.gd
/// escrits, INI de project.godot parsejable i referències coherents entre
/// els tres fitxers (main_scene → main.tscn → main.gd).
#[test]
fn frase_fes_un_joc_produeix_projecte_godot_coherent() {
    let resp = resposta_determinista("fes un joc");
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut written = runtools::write_announced_files_at(root, &resp);
    written.sort();
    assert_eq!(
        written,
        vec!["main.gd".to_string(), "main.tscn".to_string(), "project.godot".to_string()],
        "«fes un joc» ha de generar el projecte Godot de 3 fitxers"
    );

    // INI parsejable: comentaris (;), seccions [nom] i parelles clau=valor.
    let godot = std::fs::read_to_string(root.join("project.godot")).unwrap();
    let mut seccio = String::new();
    let mut claus: Vec<(String, String, String)> = Vec::new(); // (secció, clau, valor)
    for (n, linia) in godot.lines().enumerate() {
        let l = linia.trim();
        if l.is_empty() || l.starts_with(';') || l.starts_with('#') {
            continue;
        }
        if l.starts_with('[') {
            assert!(l.ends_with(']') && l.len() > 2, "secció INI invàlida (línia {}): {l}", n + 1);
            seccio = l[1..l.len() - 1].to_string();
            assert!(
                seccio.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.'),
                "nom de secció invàlid: {seccio}"
            );
            continue;
        }
        let (k, v) = l.split_once('=').unwrap_or_else(|| {
            panic!("línia INI sense clau=valor (línia {}): {l}", n + 1)
        });
        assert!(!k.trim().is_empty(), "clau buida (línia {}): {l}", n + 1);
        claus.push((seccio.clone(), k.trim().to_string(), v.trim().to_string()));
    }
    // config_version=5 (Godot 4) abans de cap secció.
    assert!(
        claus.iter().any(|(s, k, v)| s.is_empty() && k == "config_version" && v == "5"),
        "project.godot sense config_version=5: {godot}"
    );
    // L'escena principal referenciada EXISTEIX al projecte escrit.
    let main_scene = claus
        .iter()
        .find(|(s, k, _)| s == "application" && k == "run/main_scene")
        .map(|(_, _, v)| v.clone())
        .expect("project.godot sense run/main_scene");
    assert_eq!(main_scene, "\"res://main.tscn\"");
    assert!(root.join("main.tscn").is_file(), "main_scene apunta a un fitxer inexistent");

    // main.tscn: escena Godot 4 vàlida amb el script main.gd referenciat.
    let tscn = std::fs::read_to_string(root.join("main.tscn")).unwrap();
    assert!(tscn.starts_with("[gd_scene"), "main.tscn no és una escena: {tscn}");
    assert!(tscn.contains("format=3"), "escena sense format=3 (Godot 4): {tscn}");
    assert!(
        tscn.contains("[ext_resource type=\"Script\" path=\"res://main.gd\""),
        "main.tscn no referencia main.gd: {tscn}"
    );
    assert!(tscn.contains("[node name=\"Main\" type=\"Node2D\"]"), "escena sense node arrel: {tscn}");
    assert!(root.join("main.gd").is_file(), "el script referenciat no existeix");

    // main.gd: script GDScript coherent amb el node de l'escena.
    let gd = std::fs::read_to_string(root.join("main.gd")).unwrap();
    assert!(gd.contains("extends Node2D"), "main.gd no estén Node2D: {gd}");
    assert!(gd.contains("func _process(") && gd.contains("func _draw("), "script sense lògica: {gd}");
}

/// Contenció del camí real d'escriptura: rutes absolutes o amb «..» MAI
/// s'escriuen (ni dins ni fora del projecte); la resta del lot sí.
#[test]
fn escriptura_anunciada_rebutja_rutes_fora_del_projecte() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let text = "@file: ../fora.txt\n```text\nfora\n```\n@file: /tmp/absolut-fora.txt\n```text\nfora\n```\n@file: sub/dins.txt\n```text\ndins\n```\n";
    let written = runtools::write_announced_files_at(root, text);
    assert_eq!(written, vec!["sub/dins.txt".to_string()]);
    assert!(root.join("sub/dins.txt").is_file());
    assert!(!root.parent().unwrap().join("fora.txt").exists());
    assert!(!std::path::Path::new("/tmp/absolut-fora.txt").exists());
}

/// LINT de l'editor amb rutes ABSOLUTES: el frontend de vegades envia el
/// camí complet del fitxer obert. Si és SOTA el root s'ha de relativitzar i
/// verificar de veritat (abans queia a «ruta fora del projecte» i el lint
/// no s'executava mai); les absolutes FORA del root i les que porten «..»
/// es continuen rebutjant (doble xarxa amb el frontend).
#[test]
fn lint_accepta_absolutes_dins_del_root_i_rebutja_les_de_fora() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/hola.php"), "<?php echo 'hola';\n").unwrap();
    std::fs::write(root.join("src/app.js"), "const x = 1;\n").unwrap();
    let abs_php = root.join("src/hola.php").to_string_lossy().to_string();
    let abs_js = root.join("src/app.js").to_string_lossy().to_string();

    // lint_rel: absoluta dins → relativa; fora o amb «..» → None.
    assert_eq!(runtools::lint_rel(root, &abs_php).as_deref(), Some("src/hola.php"));
    assert_eq!(runtools::lint_rel(root, "src/app.js").as_deref(), Some("src/app.js"));
    assert_eq!(runtools::lint_rel(root, "/tmp/fora-del-root.php"), None);
    assert_eq!(
        runtools::lint_rel(root, &format!("{}/../fora.php", root.display())),
        None
    );

    // Camí REAL del comandament lint_paths: les absolutes dins del root es
    // verifiquen (path relatiu al resultat, sense rebuig de contenció).
    let fora = "/tmp/fora-del-root.php".to_string();
    let lint = runtools::lint_paths_blocking(root, &[abs_php.clone(), abs_js.clone(), fora.clone()]);
    assert_eq!(lint.len(), 3);
    for (r, rel) in lint.iter().zip(["src/hola.php", "src/app.js"]) {
        assert_eq!(r.path, rel, "el resultat ha de portar la ruta RELATIVA: {r:?}");
        assert!(r.ok, "lint d'una ruta absoluta dins del root ha de passar: {r:?}");
        assert!(
            !r.output.contains("fora del projecte"),
            "rebuig de contenció indegut: {r:?}"
        );
    }
    // Si les eines reals hi són, la comprovació s'ha EXECUTAT de veritat.
    if runtools::php_binary().is_some() {
        assert!(lint[0].checked, "php -l no s'ha executat: {:?}", lint[0]);
    }
    if runtools::node_binary().is_some() {
        assert!(lint[1].checked, "node --check no s'ha executat: {:?}", lint[1]);
    }
    // L'absoluta FORA del root continua sent rebutjada.
    assert!(!lint[2].ok && !lint[2].checked);
    assert!(lint[2].output.contains("fora del projecte"), "{:?}", lint[2]);
}