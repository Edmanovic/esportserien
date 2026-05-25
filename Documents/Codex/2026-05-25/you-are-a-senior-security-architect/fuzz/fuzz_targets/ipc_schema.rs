#![no_main]

use espass_shared_types::ipc::IpcPayload;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<IpcPayload>(data);
});

