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

// TODO: Add more endpoints that can be accessed with unlimited concurrency.
// /src/actix/api/retrieve_api.rs, /src/actix/api/config_search_api.rs
export const maxConcurrencyEndpoints = [
  {
    name: "search_points",
    route: "/collections/{name}/points/search",
    method: HttpMethod.POST,
  },
  {
    name: "batch_search_points",
    route: "/collections/{name}/points/search/batch",
    method: HttpMethod.POST,
  },
  {
    name: "search_point_groups",
    route: "/collections/{name}/points/search/groups",
    method: HttpMethod.POST,
  },
  {
    name: "scroll_points",
    route: "/collections/{name}/points/scroll",
    method: HttpMethod.POST,
  },
  {
    name: "get_points",
    route: "/collections/{name}/points",
    method: HttpMethod.POST,
  },
  {
    name: "get_point",
    route: "/collections/{name}/points/{id}",
    method: HttpMethod.POST,
  },
  // Add more endpoints here
];
