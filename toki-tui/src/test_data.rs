/// Test projects and activities for demo purposes
/// In production, these would come from the Milltime API

#[derive(Debug, Clone)]
pub struct TestProject {
    pub id: String,
    pub name: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TestActivity {
    pub id: String,
    pub name: String,
    pub project_id: String,
}

impl TestProject {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            code: None,
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}

impl TestActivity {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        project_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            project_id: project_id.into(),
        }
    }
}

/// Get test projects for demo user
pub fn get_test_projects() -> Vec<TestProject> {
    vec![
        TestProject::new("proj_1", "Nordic Crisis Manager").with_code("NCM"),
        TestProject::new("proj_2", "Azure DevOps Integration").with_code("ADO-INT"),
        TestProject::new("proj_3", "TUI Development").with_code("TUI"),
        TestProject::new("proj_4", "Internal Tools").with_code("TOOLS"),
        TestProject::new("proj_5", "Research & Learning"),
    ]
}

/// Get test activities for a project
pub fn get_test_activities(project_id: &str) -> Vec<TestActivity> {
    match project_id {
        "proj_1" => vec![
            TestActivity::new("act_1_1", "Backend Development", project_id),
            TestActivity::new("act_1_2", "Frontend Development", project_id),
            TestActivity::new("act_1_3", "Bug Fixes", project_id),
            TestActivity::new("act_1_4", "Code Review", project_id),
            TestActivity::new("act_1_5", "UI/UX development", project_id),
        ],
        "proj_2" => vec![
            TestActivity::new("act_2_1", "API Integration", project_id),
            TestActivity::new("act_2_2", "Webhook Setup", project_id),
            TestActivity::new("act_2_3", "Testing", project_id),
        ],
        "proj_3" => vec![
            TestActivity::new("act_3_1", "UI Design", project_id),
            TestActivity::new("act_3_2", "Feature Implementation", project_id),
            TestActivity::new("act_3_3", "Testing & Debugging", project_id),
        ],
        "proj_4" => vec![
            TestActivity::new("act_4_1", "Development", project_id),
            TestActivity::new("act_4_2", "Maintenance", project_id),
            TestActivity::new("act_4_3", "Documentation", project_id),
        ],
        "proj_5" => vec![
            TestActivity::new("act_5_1", "Learning", project_id),
            TestActivity::new("act_5_2", "Experimentation", project_id),
            TestActivity::new("act_5_3", "Proof of Concept", project_id),
        ],
        _ => vec![],
    }
}
