use toylang::run_and_capture;
#[test]
fn overflowing_arithmetic_reports_a_runtime_error() {
 let large=format!("1{}","0".repeat(200));
 for expression in [format!("{large} * {large}"),format!("{large} / 0.{}1","0".repeat(199))] {
  let error=run_and_capture(&format!("print({expression});")).unwrap_err();
  assert!(error.contains("not finite"),"{error}");
 }
 assert_eq!(run_and_capture("print(4 * 5); print(1 / 2);").unwrap(),"20\n0.5\n");
}
