# Bevy OpenGL

## WebGL 1:
Run locally with [bevy web cli](https://github.com/TheBevyFlock/bevy_cli)
`bevy run web --release --example load_gltf --open`

## Windows XP:
Tested with XP Pro SP3 32-bit on:
- VMware with Guest Additions OpenGL driver
```
GL_VENDOR   : VMware, Inc.
GL_RENDERER : Gallium 0.4 on SVGA3D; build: RELEASE;
GL_VERSION  : 2.1 Mesa 10.0 (git-5da4fa2)
```
- VirtualBox with [Mesa for windows 17.0.0](https://downloads.fdossena.com/Projects/Mesa3D/Builds/index.php)
```
GL_VENDOR   : VMware, Inc.
GL_RENDERER : Gallium 0.4 on llvmpipe (LLVM 3.7, 128 bits)
GL_VERSION  : 3.0 Mesa 17.0.0
```

Use [thunk](https://github.com/felixmaker/thunk/)
cargo install thunk-cli 

Environment Variables:
`VC_LTL` [VC-LTL-Binary](https://github.com/Chuyu-Team/VC-LTL5/releases/tag/v5.3.1)
`YY_THUNKS` [YY-Thunks-Objs](https://github.com/Chuyu-Team/YY-Thunks/releases/tag/v1.1.9)

`thunk --os xp --arch x86 -- --example load_gltf --release`
