CREATE TABLE repository_milltime_projects (
    repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    milltime_project_id TEXT NOT NULL,
    PRIMARY KEY (repository_id, milltime_project_id)
);