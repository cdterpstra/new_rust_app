use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use std::env;
use diesel::r2d2;

// Define a type alias for the connection pool.
pub type PgPool = Pool<ConnectionManager<PgConnection>>;

// Create a function to establish a database connection.
pub fn establish_connection() -> PgPool {
    // Read the database URL from the environment variables.
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in the environment");

    // Create a connection manager for PostgreSQL.
    let manager = ConnectionManager::<PgConnection>::new(database_url);

    // Create a connection pool with a maximum of 10 connections.
    // Adjust the pool size as needed for your application.
    r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create connection pool")

}
