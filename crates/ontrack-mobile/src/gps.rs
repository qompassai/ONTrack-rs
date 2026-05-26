// /qompassai/ontrack-rs/crates/ontrack-mobile/src/gps.rs
// Qompass AI — OnTrack mobile: Android GPS via JNI
// Copyright (C) 2026 Qompass AI, All rights reserved.
// -----------------------------------------------------
//! Android-only GPS access via JNI → `android.location.LocationManager`.
//!
//! `last_known()` returns `Some(Location{address:"Current Location", ...})`
//! if a recent fix is available from the GPS or NETWORK provider. Returns
//! `None` if permissions are denied or no fix is cached, allowing the
//! caller to fall back to IP-based location.

#![cfg(target_os = "android")]

use jni::objects::{JObject, JString, JValue};
use jni::JavaVM;
use ontrack_core::geocoder::Location;

pub fn last_known() -> Option<Location> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm() as *mut _) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };

    // ctx = (Context) activity
    // String svc = Context.LOCATION_SERVICE  ("location")
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

    // Try GPS provider, fall back to NETWORK.
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
