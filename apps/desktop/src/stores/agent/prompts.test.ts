/// Tests de la detecció de context WordPress (prou ESTRETA: «tema/theme»
/// genèric NO ha de disparar el flux WP) i de les plantilles reals dels 5
/// fitxers canònics (degradació OOM: mai «només README amb el pla»).
import { describe, expect, it } from "vitest";
import {
  SCAFFOLD_MIN_GENERIC,
  SCAFFOLD_MIN_WP,
  WP_THEME_CORE,
  esProjecteWordPress,
  wpThemeTemplates,
} from "./prompts";

describe("esProjecteWordPress (regex wpHint estricta)", () => {
  it("«tema/theme» genèric NO dispara el flux WordPress", () => {
    expect(esProjecteWordPress("vull un tema fosc per a la meua app")).toBe(false);
    expect(esProjecteWordPress("crea un theme per a la web del meu grup")).toBe(false);
    expect(esProjecteWordPress("fes un plugin de notes per a l'escriptori")).toBe(false);
    expect(esProjecteWordPress("una plantilla HTML per a una landing")).toBe(false);
  });

  it("context WordPress EXPLÍCIT sí que el dispara", () => {
    expect(esProjecteWordPress("crea un tema de WordPress")).toBe(true);
    expect(esProjecteWordPress("vull una botiga amb WooCommerce")).toBe(true);
    expect(esProjecteWordPress("edita el wp-content del projecte")).toBe(true);
    expect(esProjecteWordPress("un wp-theme personalitzat")).toBe(true);
  });

  it("«wp» + mot de domini (tema/plugin/plantilla) sí; «wp» sol no", () => {
    expect(esProjecteWordPress("tema wp")).toBe(true);
    expect(esProjecteWordPress("wp theme")).toBe(true);
    expect(esProjecteWordPress("un plugin per a wp")).toBe(true);
    expect(esProjecteWordPress("plantilla wp")).toBe(true);
    expect(esProjecteWordPress("instal·la wp al servidor")).toBe(false);
  });
});

describe("wpThemeTemplates (plantilles REALS, no placeholders)", () => {
  const tpls = wpThemeTemplates("Bergamota", "Botiga de cítrics", "");

  it("genera EXACTAMENT els 5 fitxers de WP_THEME_CORE", () => {
    expect(tpls.map((f) => f.path)).toEqual(WP_THEME_CORE);
    for (const f of tpls) expect(f.content.trim().length).toBeGreaterThan(50);
  });

  it("style.css porta la capçalera completa amb el nom i l'autor per defecte", () => {
    const css = tpls.find((f) => f.path === "style.css")!.content;
    expect(css).toContain("Theme Name: Bergamota");
    expect(css).toContain("Author: Joan Puigbert Carreras");
    expect(css).toContain("Text Domain: bergamota");
    expect(css).toContain("Description: Botiga de cítrics");
  });

  it("functions.php porta hooks reals amb el prefix del slug", () => {
    const fn = tpls.find((f) => f.path === "functions.php")!.content;
    expect(fn).toContain("function bergamota_setup()");
    expect(fn).toContain("add_action( 'after_setup_theme', 'bergamota_setup' )");
    expect(fn).toContain("wp_enqueue_style");
  });

  it("index/header/footer són un tema mínim que WordPress carrega", () => {
    const idx = tpls.find((f) => f.path === "index.php")!.content;
    expect(idx).toContain("have_posts()");
    expect(idx).toContain("get_header()");
    const head = tpls.find((f) => f.path === "header.php")!.content;
    expect(head).toContain("wp_head()");
    const foot = tpls.find((f) => f.path === "footer.php")!.content;
    expect(foot).toContain("wp_footer()");
  });

  it("el slug es normalitza (accents i espais) i l'autor explícit mana", () => {
    const t = wpThemeTemplates("El Meu Tema Àgil", "idea", "Autora Test");
    const css = t.find((f) => f.path === "style.css")!.content;
    expect(css).toContain("Text Domain: el-meu-tema-agil");
    expect(css).toContain("Author: Autora Test");
    const fn = t.find((f) => f.path === "functions.php")!.content;
    expect(fn).toContain("function el_meu_tema_agil_setup()");
  });

  it("una descripció amb «*/» no trenca el comentari de la capçalera", () => {
    const t = wpThemeTemplates("X", "danger */ injection", "");
    const css = t.find((f) => f.path === "style.css")!.content;
    expect(css).toContain("Description: danger * / injection");
  });

  it("un NOM amb «*/» o salts de línia no trenca cap comentari (CSS ni PHP)", () => {
    const t = wpThemeTemplates("Tema */ evil\ninjectat", "idea", "");
    const css = t.find((f) => f.path === "style.css")!.content;
    expect(css).toContain("Theme Name: Tema * / evil injectat");
    // L'únic «*/» del fitxer és el tancament legítim de la capçalera.
    expect(css.match(/\*\//g)).toHaveLength(1);
    const fn = t.find((f) => f.path === "functions.php")!.content;
    expect(fn).toContain("Funcions del tema Tema * / evil injectat.");
    expect(fn.match(/\*\//g)).toHaveLength(1);
  });

  it("un AUTOR amb «*/» o salts de línia tampoc no trenca la capçalera", () => {
    const t = wpThemeTemplates("Tema", "idea", "Autor */ mal\nbad");
    const css = t.find((f) => f.path === "style.css")!.content;
    expect(css).toContain("Author: Autor * / mal bad");
    expect(css.match(/\*\//g)).toHaveLength(1);
  });
});

describe("instruccions del nivell mínim d'OOM", () => {
  it("WP demana els 5 fitxers canònics; el genèric, els imprescindibles", () => {
    for (const p of WP_THEME_CORE) expect(SCAFFOLD_MIN_WP).toContain(p);
    expect(SCAFFOLD_MIN_WP).not.toMatch(/només un fitxer README/i);
    expect(SCAFFOLD_MIN_GENERIC).toContain("README.md");
    expect(SCAFFOLD_MIN_GENERIC).toMatch(/imprescindibles/i);
  });
});