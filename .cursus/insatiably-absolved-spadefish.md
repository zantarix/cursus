+++
"@zantarix/cursus" = "patch"
cursus-bin = "patch"
+++

Fixes npm install on Windows where `./node_modules/.bin/cursus` would print "native binary is not installed" after a successful install. Also adds `cargo binstall cursus-bin` support for fast prebuilt-binary installs from the Rust ecosystem, with glibc Linux mapped to the musl artifact automatically.
