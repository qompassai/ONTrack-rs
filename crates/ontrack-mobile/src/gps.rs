
#![cfg(target_os = "android")]

use jni::objects::{JObject, JValue};
use jni::JavaVM;
use ontrack_core::geocoder::Location;

pub fn last_known() -> Option<Location> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm() as *mut _) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };

    let svc = env.new_string("location").ok()?;
    let lm = env
        .call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&JObject::from(svc))],
        )
        .ok()?
        .l()
        .ok()?;

    for provider in ["gps", "network", "passive"] {
        let p = env.new_string(provider).ok()?;
        let loc = env
            .call_method(
                &lm,
                "getLastKnownLocation",
                "(Ljava/lang/String;)Landroid/location/Location;",
                &[JValue::Object(&JObject::from(p))],
            )
            .ok()
            .and_then(|v| v.l().ok());
        if let Some(loc) = loc {
            if !loc.is_null() {
                let lat = env.call_method(&loc, "getLatitude", "()D", &[]).ok()?.d().ok()?;
                let lng = env.call_method(&loc, "getLongitude", "()D", &[]).ok()?.d().ok()?;
                return Some(Location {
                    address: "Current Location".to_string(),
                    lat: Some(lat),
                    lng: Some(lng),
                });
            }
        }
    }
    None
}
