fastlane documentation
----

# Installation

Make sure you have the latest version of the Xcode command line tools installed:

```sh
xcode-select --install
```

For _fastlane_ installation instructions, see [Installing _fastlane_](https://docs.fastlane.tools/#installing-fastlane)

# Available Actions

## Android

### android closed

```sh
[bundle exec] fastlane android closed
```

Build ONTrack Android AAB and upload to Closed Testing (API track 'alpha')

### android internal

```sh
[bundle exec] fastlane android internal
```

Build ONTrack Android AAB and upload to Internal testing (fastest review path)

### android check_auth

```sh
[bundle exec] fastlane android check_auth
```

Validate service-account credentials only (no build, no AAB needed)

### android validate

```sh
[bundle exec] fastlane android validate
```

Build + dry-run upload (validate_only). Builds AAB then asks Google to validate it.

----

This README.md is auto-generated and will be re-generated every time [_fastlane_](https://fastlane.tools) is run.

More information about _fastlane_ can be found on [fastlane.tools](https://fastlane.tools).

The documentation of _fastlane_ can be found on [docs.fastlane.tools](https://docs.fastlane.tools).
