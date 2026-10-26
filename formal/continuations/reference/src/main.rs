mod scenarios;
use scenarios::{Scenario, finish};
fn main() {
    let asynchronous = std::env::args().any(|v| v == "async");
    let mut scenario = Scenario::new(asynchronous, false);
    let start = scenario.start();
    let value = finish(&mut scenario.id, start).unwrap();
    assert_eq!(value, scenario.answer);
    // Projection shared with the TLA+ completion witness. Private continuation
    // entry and explicit custom-lookup instrumentation are not model events.
    for event in scenario.id.trace() {
        if ["custom-lookup", "native0", "native1", "helperBody"].contains(&event.method.as_str()) {
            let method = if event.method == "custom-lookup" {
                "lookup0"
            } else {
                &event.method
            };
            println!(
                "ENTRY|{}|{}|{}|{}",
                method,
                event.receiver,
                event.selector,
                event.arguments.join(",")
            );
        }
    }
    println!("RESULT|answer");
    println!("STATS|peak_entry_depth|{}", scenario.id.peak_entry_depth());
}
