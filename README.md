# Bevy OpenGL

## WebGL 1:
Run locally with [bevy web cli](https://github.com/TheBevyFlock/bevy_cli)
`bevy run web --release --example load_gltf --open`

## Windows XP:
(Tested with XP Pro SP3 32-bit in VMware)

Use [thunk](https://github.com/felixmaker/thunk/)
cargo install thunk-cli 

Environment Variables:
`VC_LTL` [VC-LTL-Binary](https://github.com/Chuyu-Team/VC-LTL5/releases/tag/v5.3.1)
`YY_THUNKS` [YY-Thunks-Objs](https://github.com/Chuyu-Team/YY-Thunks/releases/tag/v1.1.9)

`thunk --os xp --arch x86 -- --example load_gltf --release`