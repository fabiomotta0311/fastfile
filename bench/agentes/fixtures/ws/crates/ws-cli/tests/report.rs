use ws_cli::report::summarize;

#[test]
fn soma_valores() {
    assert_eq!(summarize(&["1.50", " 2.50 "]), "total=4.00 invalid=0");
}

#[test]
fn conta_invalidos() {
    assert_eq!(summarize(&["abc", "1.00"]), "total=1.00 invalid=1");
}
