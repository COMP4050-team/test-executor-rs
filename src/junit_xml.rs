use rocket::serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(crate = "rocket::serde")]
pub struct TestSuite {
    name: String,
    pub tests: String,
    pub failures: String,
    pub errors: String,
    time: String,
    #[serde(rename = "$value")]
    children: Vec<TestSuiteChild>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(crate = "rocket::serde")]
enum TestSuiteChild {
    #[serde(rename = "properties")]
    Properties(Properties),
    #[serde(rename = "testcase")]
    TestCase(TestCase),
    #[serde(rename = "system-out")]
    SystemOut(String),
    #[serde(rename = "system-err")]
    SystemErr(String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(crate = "rocket::serde")]
struct Properties {}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(crate = "rocket::serde")]
struct TestCase {
    name: String,
    classname: String,
    time: String,
    failure: Option<Failure>,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(crate = "rocket::serde")]
struct Failure {
    message: String,
    #[serde(rename = "type")]
    failure_type: String,
    #[serde(rename = "$value")]
    value: String,
}
