extern crate diesel;
extern crate dotenv;

use diesel::prelude::*;
use diesel::pg::PgConnection;
use diesel::sql_types::Bool;
use diesel::QueryableByName;
use dotenv::dotenv;
use std::env;
use log::debug;

#[derive(QueryableByName)]
struct ExistsResult {
    #[diesel(sql_type = Bool)]
    exists: bool,
}

fn establish_connection() -> PgConnection {
    dotenv().ok();
    debug!("Database URL: {:?}", env::var("DATABASE_URL"));

    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

pub(crate) async fn check_schema() {



    let mut connection = establish_connection();

    let schema_exists: ExistsResult = diesel::sql_query("SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = 'crypto') AS exists;")
        .get_result(&mut connection)
        .expect("Error checking for schema");

    if !schema_exists.exists {
        diesel::sql_query("CREATE SCHEMA crypto;")
            .execute(&mut connection)
            .expect("Error creating schema");
        debug!("Schema 'crypto' created.");
    } else {
        debug!("Schema 'crypto' already exists.");
    }
}
