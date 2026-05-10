# Build artifacts

Release scripts and CI collect finished distributables here:

```text
releases/
├── macos/
├── windows/
└── linux/
```

Generated contents are ignored by Git. The directory structure is retained so
local builds and CI artifacts use the same predictable paths.
