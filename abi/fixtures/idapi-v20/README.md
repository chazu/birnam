# IdApi v20 native fixture

Independent C mirror of the 18-field, 144-byte arm64 macOS table. Native
initialization receives API, prior state, presence, import, export, vtable
enumeration, method observation, root registry, GC settings, payload-layout, trace-hook and finalizer pointers. The client checks
canonical runtime protocols and repeated initialization with the same borrowed
capabilities. Historical fixtures remain frozen. See design 81BO for the
extraction scope and validation status.

The removed stack-anchor setter is supplied through Native Stack v1 directly
to the host; native initialization still has twelve arguments.

`storage_v1.h` keeps Storage v1's unchanged layout with the current IdApi include.
