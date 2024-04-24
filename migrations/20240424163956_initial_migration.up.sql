-- Add up migration script here
CREATE TYPE Language AS ENUM ('ENGlISH', 'TURKISH', 'GERMAN');

CREATE TABLE Sounds (
    id SERIAL PRIMARY KEY,
    prompt VARCHAR(255) NOT NULL,
    language VARCHAR(255) NOT NULL,
    server_id VARCHAR(255) NOT NULL,
    file_name VARCHAR(225) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
