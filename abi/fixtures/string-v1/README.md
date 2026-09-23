# String protocol

The descriptor-based StringApiV1/StringStorageV1 boundary and adoption path are
retired under [Design 90](../../../docs/design/90-functional-modules.md).
The production module accepts Namespace and returns the String Object; String
Builder is implemented in the same library. See the maintained
[String message and lifetime protocol](../../../rust/birnam-string/README.md).

The production checks run through Siphon/Id/Bootstrap with
`scripts/native-message-test.sh`. Numeric text, collection and image integration
remain pending; historical descriptor-based fixtures do not validate this path.
