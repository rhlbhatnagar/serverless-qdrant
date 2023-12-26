import { Duration } from "aws-cdk-lib";
import { HttpMethod } from "aws-cdk-lib/aws-apigatewayv2";
import { Runtime, Architecture, Code } from "aws-cdk-lib/aws-lambda";
import { RetentionDays } from "aws-cdk-lib/aws-logs";

export const commonLambdaParams = {
  runtime: Runtime.PROVIDED_AL2023,
  handler: "not.required",
  architecture: Architecture.ARM_64,
  timeout: Duration.seconds(30),
  logRetention: RetentionDays.ONE_MONTH,
  memorySize: 3000, // 3 GB memory
};
// TODO: Add more endpoints that write data on the index.
// /src/actix/api/update_api.rs, /src/actix/api/collection_api.rs
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
