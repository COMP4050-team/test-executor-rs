# test-executor-rs

This repo contains the source for the test execution service. It is responsible for:
1. Pulling down Processing project and JUnit test files from S3
2. Converting the Processing projects to Java projects
3. Running the JUnit tests against the converted Java projects
4. Uploading the results back to S3

## Development

1. [Install rust](https://www.rust-lang.org/learn/get-started)
2. Run with:
    ```bash
    cargo run
    ```
3. The service is accessible at http://localhost:8080
4. Populate S3 with the correct files and directory structure. This can be done by running the `scripts/add_dummy_data.py` script in the API repo.
5. Send a test request such as:
    ```bash
    curl -XPOST http://127.0.0.1:8080/ -d '{"s3KeyTestFile": "Unit 1/Assignment 1/Tests/1/Test.java", "s3KeyProjectFile": "Unit 1/Assignment 1/Projects"}
    ```

    This assumes the existing file structure and files are already in S3
