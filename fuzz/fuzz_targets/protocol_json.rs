#![no_main]

use libfuzzer_sys::fuzz_target;
use mk01_server::protocol::ClientEvent;

fuzz_target!(|data: &[u8]| {
    // The public WebSocket boundary must reject arbitrary bytes without panicking,
    // allocating unbounded recursive values, or accepting unknown envelope fields.
    let _ = serde_json::from_slice::<ClientEvent>(data);
});
