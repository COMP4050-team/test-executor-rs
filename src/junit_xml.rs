use rocket::serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(crate = "rocket::serde")]
pub struct TestSuite {
    name: String,
    pub tests: String,
    pub failures: String,
    pub errors: String,
    time: String,
    #[serde(rename = "$value")]
    pub children: Vec<TestSuiteChild>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(crate = "rocket::serde")]
pub enum TestSuiteChild {
    #[serde(rename = "properties")]
    Properties(Properties),
    #[serde(rename = "testcase")]
    TestCase(TestCase),
    #[serde(rename = "system-out")]
    SystemOut(String),
    #[serde(rename = "system-err")]
    SystemErr(String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(crate = "rocket::serde")]
pub struct Properties {}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(crate = "rocket::serde")]
pub struct TestCase {
    pub name: String,
    pub classname: String,
    pub time: String,
    pub failure: Option<Failure>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(crate = "rocket::serde")]
pub struct Failure {
    pub message: String,
    #[serde(rename = "type")]
    pub failure_type: String,
    #[serde(rename = "$value")]
    pub value: String,
}
