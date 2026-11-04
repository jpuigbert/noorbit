---
name: WordPress (temes i connectors)
description: Crear i editar temes, connectors i blocs de funcions per WordPress — PHP, WP-CLI i compatibilitat amb Gutenberg.
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

No toquis wp-admin ni la base de dades en viu sense permís explícit de l'usuari.
