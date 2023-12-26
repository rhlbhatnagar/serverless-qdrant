import { Duration } from "aws-cdk-lib";
import { HttpMethod } from "aws-cdk-lib/aws-apigatewayv2";
import { Runtime, Architecture, Code } from "aws-cdk-lib/aws-lambda";
import { RetentionDays } from "aws-cdk-lib/aws-logs";

export const readLambdaParams = {
  runtime: Runtime.PROVIDED_AL2023,
  handler: "not.required",
  architecture: Architecture.ARM_64,
  timeout: Duration.seconds(30),
  logRetention: RetentionDays.ONE_MONTH,
  memorySize: 3000, // 3 GB memory
};

export const writeLambdaParams = {
  ...readLambdaParams,
  // IMP: On fresh AWS accounts the min and max lambda concurrency limit is 10,
  // If that's the case won't be able to reserve a concurrent execution for this lambda as it will
  // take your free lambda limit to below your minimum limit.
  // (read more: https://stackoverflow.com/questions/73988837/aws-specified-concurrentexecutions-for-function-decreases-accounts-unreservedc)
  // So you might need to request an increase in your max concurrency limit from AWS service quotas.
  // https://console.aws.amazon.com/servicequotas/home
  reservedConcurrentExecutions: 1, // Write lambda has a forced concurrency of 1,
};

// Write enspoints ,that will we served with the write lambda, which will have only 1 concurrent executions.
// This is so that there aren't any race conditions while updating the index on the file system
// if multiple lambda instances start writing on it.
// TODO: Add more endpoints that write data on the index.
// You can see them on the /src/actix/api/update_api.rs
// and on /src/actix/api/collection_api.rs
export const writeEndpoints = [
  {
    name: "upsert_points",
    route: "/collections/{name}/points",
    method: HttpMethod.PUT,
  },
  {
    name: "delete_points",
    route: "/collections/{name}/points/delete",
    method: HttpMethod.DELETE,
  },
  // Add more endpoints here
];
