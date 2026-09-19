# Frontend third-party notices

The source dependency versions are locked in `package-lock.json`. These upstream
license texts accompany the fonts and libraries used by the built browser UI.
Vite copies `public/licenses/` into `dist/licenses/`; distribute that directory
with the assets, not just the JavaScript and CSS. Preserve notices when updating
dependencies. Development tools retain their own licenses in their packages.

| Package | Recorded version | Supplied notice |
| --- | --- | --- |
| @fontsource/barlow | 5.3.0 | [OFL-1.1](public/licenses/fontsource-barlow.txt) |
| @fontsource/barlow-condensed | 5.3.0 | [OFL-1.1](public/licenses/fontsource-barlow-condensed.txt) |
| @fontsource/rajdhani | 5.3.0 | [OFL-1.1](public/licenses/fontsource-rajdhani.txt) |
| @xterm/xterm | 6.0.0 | [MIT](public/licenses/xterm-xterm.txt) |
| @xterm/addon-fit | 0.11.0 | [MIT](public/licenses/xterm-addon-fit.txt) |
| dompurify | 3.4.15 | [Apache-2.0](public/licenses/dompurify.txt) |
| highlight.js | 11.12.0 | [BSD-3-Clause](public/licenses/highlight.js.txt) |
| marked | 18.0.12 | [MIT and included notices](public/licenses/marked.txt) |
| svelte | 5.57.0 | [MIT](public/licenses/svelte.txt) |
| clsx | 2.1.1 | [MIT](public/licenses/clsx.txt) |
| esm-env | 1.2.2 | [MIT](public/licenses/esm-env.txt) |

The DOMPurify package declares Apache-2.0 OR MPL-2.0; its supplied LICENSE is
reproduced here. Notices are copied from the installed, lockfile-selected package
without modifying the upstream copyright statements. A dependency update should
refresh the corresponding notice and this version table before distribution.
