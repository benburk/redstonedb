// The 3D view on a build's page, the details page, and a diff: schematic-
// renderer draws the file at the canvas's `data-src` with the vanilla
// resource pack it publishes alongside its own build. The page's import map
// points the renderer's own imports, three and nucleation, at the same CDN.
// A diff's canvas also names its older side in `data-before`, and each
// `data-side` button swaps the side drawn for its own, the camera staying
// where it is.
const CDN = "https://cdn.jsdelivr.net/npm";
const RENDERER = `${CDN}/schematic-renderer@1.6.1/dist`;
const { SchematicRenderer } = await import(`${RENDERER}/schematic-renderer.es.js`);
const canvas = document.getElementById("viewer");
const status = document.getElementById("viewer-status");
new SchematicRenderer(canvas, {}, {
  vanilla: () => fetch(`${RENDERER}/pack.zip`).then((r) => r.blob()),
}, {
  enableDragAndDrop: false,
  callbacks: {
    onRendererInitialized: async (renderer) => {
      const manager = renderer.schematicManager;
      try {
        await manager.loadSchematicFromURL(canvas.dataset.src, "after");
        renderer.cameraManager.focusOnSchematics();
        status.hidden = true;
        if (canvas.dataset.before) {
          sides(manager);
        }
      } catch (error) {
        status.hidden = false;
        status.textContent = `Could not load the schematic: ${error.message}`;
      }
    },
  },
});

// The diff's buttons, each drawing its side in place of the other, one
// swap at a time.
function sides(manager) {
  const buttons = document.querySelectorAll("[data-side]");
  const files = { after: canvas.dataset.src, before: canvas.dataset.before };
  let shown = "after";
  for (const button of buttons) {
    button.disabled = false;
    button.addEventListener("click", async () => {
      const side = button.dataset.side;
      if (side === shown) return;
      for (const other of buttons) other.disabled = true;
      try {
        await manager.removeSchematic(shown);
        await manager.loadSchematicFromURL(files[side], side, { focused: false });
        shown = side;
        for (const other of buttons) {
          other.setAttribute("aria-pressed", String(other.dataset.side === side));
        }
      } catch (error) {
        status.hidden = false;
        status.textContent = `Could not load the schematic: ${error.message}`;
      } finally {
        for (const other of buttons) other.disabled = false;
      }
    });
  }
}
