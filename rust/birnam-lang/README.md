# Lang over Id

Bootstrap loads `libbirnam_lang` with Namespace after Peg. It builds the existing
Birnam grammar entirely through current Peg/Pattern/Grammar messages, retaining
all construction intermediates through Roots. It publishes Lang, LangVt and
BirnamGrammar only after construction. Repeated loading preserves the instance.

Lang `parse:` takes a source String and answers CST or Error; `grammar` answers
its grammar Object; `furthestFail` answers the grammar's current Integer offset.
The grammar policy is retained in `grammar.rs`, including threaded messages,
lexical bindings and Blocks. There is no process-global grammar or native API table.
The production compiler test exercises this parser through Bootstrap.
