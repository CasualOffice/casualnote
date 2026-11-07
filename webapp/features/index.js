// Each feature is one file exporting `install(api)`, which adds to `api.ext`:
//   views: { id: { label, render(contentEl) } }   a full view, opened with api.openView(id)
//   blocks: { kind: block => element }            how a custom block kind draws
//   slash: [{ gl, label, run(block) }]            entries in the / menu
//   pageMenu: [meta => items]                     entries in the page's ... menu
//   panels: [page => void]                        called when the side panel refreshes
//   settings: [container => void]                 sections on the Settings page
//   textHooks: [(block, node, textBeforeCaret) => handled]   runs as text changes
//   nav: [{ id, icon, label, run() }]             sidebar buttons
//   onChange: [op => void]                        called after every operation
//   pageView: page => boolean                     draw this page yourself, return true
// A feature that fails to load must not take the editor down.
const FEATURES = ["links", "database", "canvas", "knowledge", "package", "assistant", "desktop", "sync"];

export async function loadFeatures(api) {
  // Before any feature can edit: the log saves from this engine, whichever features load.
  api.store.bind?.(api.engine);
  for (const name of FEATURES) {
    document.head.append(Object.assign(document.createElement("link"), { rel: "stylesheet", href: `features/${name}.css` }));
    try {
      (await import(`./${name}.js`)).install(api);
    } catch (err) {
      console.warn(`feature ${name} did not load`, err);
    }
  }
}
