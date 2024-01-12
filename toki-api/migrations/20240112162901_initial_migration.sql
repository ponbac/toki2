-- Create repositories table
CREATE TABLE repositories
(
    id SERIAL PRIMARY KEY,
    organization VARCHAR(255) NOT NULL,
    project VARCHAR(255) NOT NULL,
    repo_name VARCHAR(255) NOT NULL,
    token VARCHAR(255) NOT NULL
);

