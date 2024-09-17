# sdl3-sys (preview release)

This is a preview release of the `sdl3-sys` SDL 3 bindings for Rust.

Please note that SDL 3 itself is still unstable and may get breaking changes
at any point.

Also, `sdl3-sys` itself is still unstable and incomplete. The generator that
parses and generates these bindings is new and may have bugs.

Known issues:
- Most function-like defines aren't generated yet
- There's no option to build and/or link the SDL library itself yet
- Satellite libraries (mixer, image, ttf) aren't available yet
- There are no tests yet, except for static asserts translated from the
  original headers
- Some less common targets are missing detection or features to enable
  corresponding SDL features
- Documentation could be formatted better

That said, please try it out and let me know if you have any comments!