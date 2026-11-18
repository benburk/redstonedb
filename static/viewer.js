// The 3D view on a build's page, the details page, and a diff: schematic-
// renderer draws the file at the canvas's `data-src` with the vanilla
// resource pack it publishes alongside its own build. The page's import map
// points the renderer's own imports, three and nucleation, at the same CDN.
// A diff's canvas also names its older side in `data-before`, and each
// `data-side` button swaps the side drawn for its own, the camera staying
// where it is. The `data-camera` buttons choose between orbiting the build
// and flying through it, first-person, once a click locks the pointer, with
// Minecraft's keys: Space rises and Shift sinks.
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
        camera(renderer);
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

// Flying's keys, Minecraft's but for sprint: Shift sinks, as in the game,
// and Ctrl, the game's sprint, would close the tab with W.
const KEYBINDS = {
  forward: "KeyW",
  backward: "KeyS",
  left: "KeyA",
  right: "KeyD",
  up: "Space",
  down: "ShiftLeft",
  sprint: "KeyQ",
};
// The renderer's overlay, asking for the click that starts flying, names its
// own keys, so it is told these.
const OVERLAY = `
  <div style="font-size: 24px; margin-bottom: 12px;">Click to fly</div>
  <div style="font-size: 14px; color: rgba(255,255,255,0.7);">
    <div>WASD to move</div>
    <div>Space up, Shift down</div>
    <div>Q to sprint</div>
    <div>Esc to stop</div>
  </div>`;

// The camera's buttons, each switching to its own mode. The renderer draws
// once a second while it thinks nothing moves, and only a click, scroll, or
// touch wakes it, so flying with the mouse still would move a step a second;
// while the pointer is locked to fly, it draws every frame.
function camera(renderer) {
  const cameras = renderer.cameraManager;
  cameras.on("flyControlsLocked", () => renderer.setContinuous("fly", true));
  cameras.on("flyControlsUnlocked", () => renderer.setContinuous("fly", false));
  const buttons = document.querySelectorAll("[data-camera]");
  for (const button of buttons) {
    button.disabled = false;
    button.addEventListener("click", () => {
      const fly = button.dataset.camera === "fly";
      if (fly === cameras.isFlyControlsEnabled()) return;
      if (fly) {
        cameras.enableFlyControls();
        cameras.setFlyControlsSettings({ keybinds: KEYBINDS });
        const overlay = cameras.flyControls?.overlayElement?.firstElementChild;
        if (overlay) overlay.innerHTML = OVERLAY;
      } else {
        cameras.disableFlyControls();
      }
      for (const other of buttons) {
        other.setAttribute("aria-pressed", String(other === button));
      }
    });
  }
}

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
