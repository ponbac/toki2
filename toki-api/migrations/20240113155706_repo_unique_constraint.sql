ALTER TABLE repositories
ADD CONSTRAINT unique_project_repo_org
UNIQUE (project, repo_name, organization);
