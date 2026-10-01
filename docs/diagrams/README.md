# Runtime diagrams

[Birnam dynamic startup](birnam-startup.png) illustrates the current production
path through 81CA: Siphon/host, pre-Id dependency preparation, native module
initialization, the Birnam bootstrap, and program/image execution. Colors identify
implementation language and image recovery. It depicts ordinary startup; compile-only
and `BIRNAM_NO_STDLIB` skip the authored bootstrap.

The source is [Graphviz DOT](birnam-startup.dot); [SVG](birnam-startup.svg) remains
scalable, while [PNG](birnam-startup.png) is rendered at 2200 pixels wide.
Re-render from the repository root with Graphviz and librsvg:

```sh
dot -Tsvg docs/diagrams/birnam-startup.dot -o docs/diagrams/birnam-startup.svg
rsvg-convert -w 2200 docs/diagrams/birnam-startup.svg -o docs/diagrams/birnam-startup.png
```

Source authority: [`startup.rs`](../../rust/birnam-host/src/startup.rs), especially
`load_floor`, `SCRIPT_MODULES`, `load_script_modules`, `start`,
`bootstrap_self_hosted` and `load_self_hosted`; command entry in
[`birnam.rs`](../../rust/birnam-host/src/bin/birnam.rs). The inventory and boundary
explanations are in [81A](../design/81a-current-bootstrap-and-runtime-layers.md).
When changing startup, update the diagram's exact native initialization list as
well as its phase labels. The second `module_init` argument is prior state;
the remaining ten arguments after IdApi and prior state are explicit capabilities.
