# REST interface for PG

Much like [postgrest](https://postgrest.org/en/stable/), pg_api_muscle is a REST interface for postgres. It provides an ad-hoc way to 

- query a postgres database with GET requests and parameters (localhost/todo?item=4 will give you todo item 4 as JSON),
- update the database with PATCH,
- insert into the database with POST,
- delete from the database with DELETE

The reason for me to develop an alternative to postgrest (which I have been using for years) is that I wanted to avoid a reverse proxy that filters my requests. Also I wanted to experiment with Rust.

# Interface with face check

## Unlike [postgrest](https://postgrest.org/en/stable), pg_api_muscle 

+ validates the request (DELETE ./todo *without* any parameters will be rejected if pg_api_muscle is so configured)
+ serves static files,
+ provides https access

Pg_muscle_api does *not*

- attempt to analyze the database to deliver nested JSON items; 
- provide a sophisticated role management

## Like [postgrest](https://postgrest.org/en/stable),

- authentication tokens are supported.

