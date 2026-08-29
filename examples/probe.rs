fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let model = oop_worm::scan::scan(&src);

    for class in &model.classes {
        println!(
            "class {} members={:?}",
            class.name,
            class
                .members
                .iter()
                .map(|(k, v)| (k.clone(), v.private))
                .collect::<Vec<_>>(),
        );
    }

    for f in oop_worm::lints::findings(&src, &model) {
        println!("{} {:?} {}", f.lint, f.span, f.message);
    }
}
