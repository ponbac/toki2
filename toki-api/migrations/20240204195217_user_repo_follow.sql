-- Junction table for many-to-many relationship between users and repositories
CREATE TABLE user_repositories
(
    user_id INT NOT NULL,
    repository_id INT NOT NULL,
    PRIMARY KEY (user_id, repository_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (repository_id) REFERENCES repositories (id) ON DELETE CASCADE
);
