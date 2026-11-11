---
name: WordPress (temes i connectors)
description: Crear i editar temes, connectors i blocs de funcions per WordPress — PHP, WP-CLI, compatibilitat amb Gutenberg i vendre temes a themes.thepuigberts.com.
---

# Com programar per WordPress amb NoOrbit

1. **Reconeix el tipus de treball**:
   - **Tema**: `style.css` + `functions.php` + plantilles PHP (`index.php`,
     `single.php`, `sidebar.php`...). Els temes moderns de blocs porten
     `theme.json` + carpetes `templates/` i `parts/` amb HTML.
   - **Connector**: carpeta amb un fitxer PHP principal amb capçalera:
     ```php
     <?php
     /* Plugin Name: ElMeuConnector
        Description: Fa el que el client demana.
        Version: 0.1 */
     ```
   - **Retocs d'un tema existent**: sempre child theme, mai el tema pare directament.
2. **Patrons que cal seguir**:
   - Contracte: mai s'edita el nucli (`wp-includes`, `wp-admin`).
   - Neteges l'entrada (input): `sanitize_text_field()`, `absint()`; escaps la sortida:
     `esc_html()`, `esc_url()`, `esc_attr()`.
   - Base de dades sempre amb `$wpdb->prepare()` — mai SQL en mà.
   - Scripts i CSS: `wp_enqueue_style/script` dins
     `add_action('wp_enqueue_scripts', fn)`; mai etiquetes <link>/<script> dures.
   - Blocs Gutenberg: `register_block_type` dins `add_action('init', ...)`; o
     `@wordpress/scripts` (npm) si el bloc porta React/JSX.
3. **Provar amb WP-CLI** (si hi ha entorn): `wp plugin activate`,
   `wp theme list`, `wp rewrite flush`.
   Si no hi ha WordPress instal·lat: proposa `wp core download` o
   `composer create-project roots/bedrock`, i ho munta amb la Vista prèvia.
4. **Seguretat**: nonces (`wp_nonce_field` + `check_ajax_referer`) per al
   AJAX; comprovacions de capacitat (`current_user_can('edit_posts')`).
5. **Escriptura**: fitxers complets amb «@file: ruta relativa» per què el
   projecte els aplica sols a l'estructura real.

6. **Vendre el tema a themes.thepuigberts.com** (si el client vol vendre'l):
   - El paquet ha de passar la validació del marketplace. Fes-lo així:
     - `style.css` amb la capçalera completa: `Theme Name`, `Version`,
       `Description`, `Requires at least`, `Requires PHP`. El nom i la
       versió d'aquí són els que es mostren a la fitxa.
     - `screenshot.png` de 1200×900 a l'arrel del tema (és la imatge de la fitxa).
     - ZIP amb tots els fitxers del tema dins **una sola carpeta**.
     - Autosuficient: cap plugin extern obligatori, res de dependències
       de pagament; opcions, plantilles i funcions dins del propi tema.
     - Codi net: escapa la sortida, sanititza l'entrada, sense `wp-admin`.
   - Publicar-lo (hi ha revisió humana, no es publica a l'instant):
     1. Compte a themes.thepuigberts.com → tauler → pestanya **«Vendes»** →
        «Començar a vendre» (activa el rol de venedor).
     2. Tauler → Vendes → **«Pujar Tema»** (dashboard ?tab=sales&selltab=my-upload):
        títol, descripció (ca/es/en/fr), categoria, ZIP, captures, preu
        i versió (ha de coincidir amb la capçalera del style.css).
     3. Declaració d'autoria + acceptar els termes → «Pujar i posar a la
        venda» → queda **«En revisió»** fins que l'equip l'aprova.
   - Actualitzar: edita el tema al tauler i puja un ZIP nou amb la versió
     incrementada; els compradors reben l'avís automàticament.
   - Guia completa del venedor: https://themes.thepuigberts.com/vendre-temes/

No toquis wp-admin ni la base de dades en viu sense permís explícit de l'usuari.
