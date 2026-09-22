# Legacy Runtime table C mirror

This header mirrors the still-used 18-field Runtime table. It remains for the
production native fixtures while Design 84 replaces the table with Id messages.
It is not the Id protocol and does not define the native module entry.

The old aggregate `id_init`/`module_init` client and module programs have been
deleted. All installed-native acceptance uses
[the Id protocol](../id-protocol-v1/README.md) and the production Bootstrap path.
See [Design 84](../../../docs/design/84-message-only-native-modules.md) for the
remaining deletion work. Historical clients can be recovered from `eb219997`.

`storage_v1.h` remains the C mirror consumed by production storage fixtures.
