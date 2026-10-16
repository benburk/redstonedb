// The 3D view on a build's page and the details page: schematic-renderer
// draws the file at the canvas's `data-src` with the vanilla resource pack
// it publishes alongside its own build. The page's import map points the
// renderer's own imports, three and nucleation, at the same CDN.
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
      try {
        await renderer.schematicManager.loadSchematicFromURL(canvas.dataset.src, "schematic");
        renderer.cameraManager.focusOnSchematics();
        status.hidden = true;
      } catch (error) {
        status.textContent = `Could not load the schematic: ${error.message}`;
      }
    },
  },
});
