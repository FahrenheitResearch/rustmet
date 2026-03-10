# rustmet-wasm

WebAssembly bindings for the rustmet GRIB2 parser. Parse and visualize GRIB2 weather data directly in the browser.

## Building

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/):

```bash
cd crates/rustmet-wasm
wasm-pack build --target web
```

This produces a `pkg/` directory containing the `.wasm` binary and JS/TS bindings.

## Usage

```javascript
import init, { WasmGribFile, renderToRgba } from './pkg/rustmet_wasm.js';

async function main() {
    // Initialize the WASM module
    await init();

    // Fetch a GRIB2 file
    const response = await fetch('data.grib2');
    const bytes = new Uint8Array(await response.arrayBuffer());

    // Parse it
    const grib = new WasmGribFile(bytes);
    console.log(`Messages: ${grib.messageCount()}`);

    // Get inventory (JSON array of all messages)
    const inventory = JSON.parse(grib.inventory());
    console.log(inventory);

    // Get detailed info for a single message
    const info = JSON.parse(grib.messageInfo(0));
    console.log(`${info.parameter} (${info.units}) at ${info.level}`);

    // Decode the raw data values
    const values = grib.values(0);
    console.log(`Grid: ${info.nx} x ${info.ny}, ${values.length} points`);

    // Get lat/lon coordinates
    const lats = grib.lats(0);
    const lons = grib.lons(0);

    // Render to RGBA pixels for a canvas
    const rgba = renderToRgba(grib, 0, 'turbo', 250, 320);
    const imageData = new ImageData(
        new Uint8ClampedArray(rgba.buffer),
        info.nx,
        info.ny
    );
    const ctx = document.getElementById('canvas').getContext('2d');
    ctx.putImageData(imageData, 0, 0);

    // Clean up
    grib.free();
}

main();
```

## Limitations

- **JPEG2000 (Template 5.40)**: Not supported in WASM because the openjp2 C library
  cannot be compiled to WebAssembly. GRIB2 files using JPEG2000 compression will
  return an error when unpacking those specific messages. Templates 5.0 (simple),
  5.2/5.3 (complex), and 5.41 (PNG) all work.

- **No network access**: The download client and cache are not available in the WASM
  build. Fetch GRIB2 data using the browser's `fetch()` API and pass the bytes in.

## Colormaps

The `renderToRgba` function supports these colormaps:

- `viridis` (default) - perceptually uniform, dark purple to yellow
- `turbo` - rainbow-like, good for weather data
- `inferno` - dark to bright, black-purple-red-yellow
- `coolwarm` - diverging blue-white-red
- `grayscale` / `gray` - simple grayscale

## npm package

To publish as an npm package:

```bash
wasm-pack build --target bundler --scope your-scope
cd pkg
npm publish --access public
```
