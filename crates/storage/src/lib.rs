// #[cfg(tests)]
mod tests {
    use std::env;

    use libs::lazy_static::lazy_static;

    use libs::cozo::DbInstance;

    lazy_static! {
        static ref TEST_DB: DbInstance = {
            let path = "_test_db";
            let db_kind = env::var("COZO_TEST_DB_ENGINE").unwrap_or("mem".to_string());
            println!("Using {db_kind} engine");

            let db = DbInstance::new(&db_kind, path, Default::default()).unwrap();
            db
        };
    }

    #[test]
    fn do1() {
        let script = "?[a] := a in [1, 2, 3]";
        let result = TEST_DB.run_script(script, Default::default()).unwrap();
        println!("{:?}", result);
    }
}
