# ONTrack — One-time Play Console Bootstrap

`fastlane supply` (the `upload_to_play_store` action) **cannot create a new
app on Google Play.** The Play Developer Publishing API v3 only updates apps
that already exist. The first AAB must be uploaded manually through the
Play Console UI.

This is the cause of:

```
Google Api Error: Invalid request - Package not found: com.qompassai.ontrack.
```

See the [fastlane docs](https://docs.fastlane.tools/actions/upload_to_play_store/#quick-start)
("Before using *supply* to connect to Google Play Store, you'll need to set
up your app manually first by uploading at least one build to Google Play
Store.") and [fastlane/fastlane#14686](https://github.com/fastlane/fastlane/issues/14686).

## Run this once, then never again

### 1. Create the app on Play Console

1. Sign in to <https://play.google.com/console> as **phaedrusflow**.
2. **Create app** ▸ fill in:
   - **App name:** ONTrack
   - **Default language:** English (United States)
   - **App or game:** App
   - **Free or paid:** Free
   - Accept both declarations and click **Create app**.
3. After the app is created, open **Dashboard ▸ Set up your app** and locate
   **App content** + **Store listing** (you can fill these in later; they
   are not required to register the package name).

> The package name is bound to the app when the **first AAB is uploaded**,
> not at app creation. So you must do step 2 below before fastlane works.

### 2. Upload the first AAB manually

Build the signed AAB on your workstation:

```bash
cd ~/.GH/Qompass/ONTrack-rs
./scripts/build-android.sh
ls crates/ontrack-mobile/android/app/build/outputs/bundle/release/app-release.aab
```

Then in Play Console:

1. **Testing ▸ Internal testing** (left sidebar)
2. **Create new release**
3. **App bundles ▸ Upload** → drag in `app-release.aab`
4. Add a one-line release note ("Initial bootstrap upload.") and click
   **Next ▸ Save**. You do NOT need to roll out yet — saving a draft is
   enough to register the package name with the API.

The package name `com.qompassai.ontrack` is now bound to this app on the
API side. All future uploads can go through fastlane.

### 3. Wire up the service account (probably already done)

If you haven't already:

1. Cloud Console → IAM → Service Accounts → create
   `fastlane-supply@<project>.iam.gserviceaccount.com`.
2. Keys ▸ Add key ▸ JSON → download to
   `~/.config/fastlane/google-play-ontrack.json` (path is hardcoded in
   `fastlane/Appfile`).
3. Play Console ▸ Users and permissions ▸ Invite new users → paste the
   service-account email → grant **Admin (all permissions)** or at minimum
   the Release section permissions.

Verify the JSON is valid:

```bash
fastlane run validate_play_store_json_key \
    json_key:$HOME/.config/fastlane/google-play-ontrack.json
```

### 4. Now fastlane will work

```bash
cd ~/.GH/Qompass/ONTrack-rs

# Dry-run — validates AAB + credentials without uploading. Run this first.
fastlane android validate

# Real upload — Closed Testing (alpha track), draft release
fastlane android closed

# Or Internal Testing (fastest, no review required)
fastlane android internal
```

## Track-name reference

| Play Console UI    | Play API track name |
|--------------------|--------------------:|
| Production         | `production`        |
| Open testing       | `beta`              |
| Closed testing     | `alpha`             |
| Internal testing   | `internal`          |

If you create a **custom closed-testing track** in Console (e.g. "qompass-private"),
its API name is the exact lowercased string you typed in Console. Pass it via:

```bash
FASTLANE_TRACK=qompass-private fastlane android closed
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Package not found: com.qompassai.ontrack` | App not created on Console or no AAB uploaded yet | Follow §1 + §2 above |
| `Unable to find the requested track - 'closed'` | Used UI name instead of API name | Use `internal` / `alpha` / `beta` / `production` |
| `forbidden: APK has the wrong package name` | `applicationId` in gradle ≠ `package_name` in Appfile | Both must be `com.qompassai.ontrack` |
| `Google Api Error: applicationNotFound` | Service-account JSON belongs to a different Cloud project than the app | Recreate the JSON in the project linked to your Play developer account |
| `apksNotAllowed: This Edit cannot upload APKs because Android App Bundles have been added.` | Trying to upload an APK after an AAB was uploaded | Use `aab:` only, set `skip_upload_apk: true` (already done) |

## Why this matters

The first manual upload is a Play policy thing — Google wants a human to
acknowledge the package-name binding before granting API write access.
Once done, every subsequent build can ship through CI without touching
the Console.
