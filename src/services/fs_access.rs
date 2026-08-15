use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;

// JS-Implementierung
#[wasm_bindgen(inline_js = r#"
export async function pickFile() {
  const [handle] = await window.showOpenFilePicker({ multiple: false });
  return handle;
}
export function hasOpenFilePicker() {
  return !!window.showOpenFilePicker;
}
export async function simpleInputPick() {
  return new Promise((resolve, reject) => {
    const inp = document.createElement('input');
    inp.type = 'file';
    inp.onchange = () => {
      if (inp.files && inp.files.length > 0) resolve(inp.files[0]);
      else reject(new Error('no file selected'));
    };
    // Muss im Click-Stack ausgelöst werden; wir rufen es direkt aus dem Button-Handler
    inp.click();
  });
}
export async function getFileFromHandle(handle) {
  return await handle.getFile();
}
// Gibt 'granted'|'prompt'|'denied' zurück – wir wandeln in Rust zu bool
export async function ensureReadPermission(handle) {
  if (handle.queryPermission && handle.requestPermission) {
    let s = await handle.queryPermission({ mode: 'read' });
    if (s !== 'granted') s = await handle.requestPermission({ mode: 'read' });
    return s; // string
  }
  return 'granted';
}
export async function sliceText(blob, start, end) {
  const part = blob.slice(start, end);
  return await part.text();
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    pub async fn pickFile() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    pub async fn getFileFromHandle(handle: &JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    pub async fn ensureReadPermission(handle: &JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    pub async fn sliceText(blob: &JsValue, start: f64, end: f64) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    pub fn hasOpenFilePicker() -> Result<bool, JsValue>;
    #[wasm_bindgen(catch)]
    pub async fn simpleInputPick() -> Result<JsValue, JsValue>;
}

/// Rust-Hilfsfunktion: JS-String -> bool
#[allow(dead_code)]
pub async fn ensure_read_permission(handle: &JsValue) -> bool {
    match ensureReadPermission(handle).await {
        Ok(v) => v.as_string().map(|s| s == "granted").unwrap_or(false),
        Err(_) => false,
    }
}