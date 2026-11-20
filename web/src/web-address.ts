/**
 * The Web view's address of a page imported into an agent's folder (Import…): `@imported/<name>`,
 * served from its `.prism/web/` (the core's `web_import::IMPORTED`). Alone here: the modules that
 * need it load one another's state otherwise.
 */
export const IMPORTED = '@imported'
