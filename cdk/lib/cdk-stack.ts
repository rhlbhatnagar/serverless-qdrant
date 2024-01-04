import { Construct } from "constructs";
import { HttpLambdaIntegration } from "aws-cdk-lib/aws-apigatewayv2-integrations";

import {
  Code,
  Function,
  FileSystem as LambdaFilesystem,
} from "aws-cdk-lib/aws-lambda";
import { FileSystem, AccessPoint } from "aws-cdk-lib/aws-efs";

import { Stack, StackProps, RemovalPolicy, CfnOutput } from "aws-cdk-lib";
import { HttpApi, HttpRoute, HttpRouteKey } from "aws-cdk-lib/aws-apigatewayv2";
import { Vpc } from "aws-cdk-lib/aws-ec2";
import { commonLambdaParams, maxConcurrencyEndpoints } from "./config";

export class QdrantLambdaStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    const vpc = new Vpc(this, "LambdaVpc", { natGateways: 0 });

    // Create a new EFS filesystem
    const fileSystem = new FileSystem(this, "LambdaEfs", {
      vpc,
      removalPolicy: RemovalPolicy.DESTROY,
    });

    const accessPoint = new AccessPoint(this, "AccessPoint", {
      fileSystem,
      path: "/export/lambda",
      createAcl: {
        ownerUid: "1001",
        ownerGid: "1001",
        permissions: "750",
      },
      posixUser: {
        uid: "1001",
        gid: "1001",
      },
    });

    // IMP: On fresh AWS accounts the min concurrency limit = max lambda concurrency = 10,
    // If that's the case won't be able to reserve a concurrent execution for this lambda
    // as this takes your free lambda limit (max - reserved) < min.

    // So you might need to request in your max concurrency limit.
    // https://console.aws.amazon.com/servicequotas/home

    const singleConcurrencyLambda = new Function(
      this,
      "SingleConcurrencyLambda",
      {
        ...commonLambdaParams,
        // This lambda has a forced concurrency of 1. So don't run into race conditions on the network file system.
        reservedConcurrentExecutions: 1,
        code: Code.fromAsset("../target/lambda/main_lambda/bootstrap.zip"),
        filesystem: LambdaFilesystem.fromEfsAccessPoint(
          accessPoint,
          "/mnt/efs"
        ),
        vpc: vpc,
      }
    );

    const singleConcurrencyIntegraiton = new HttpLambdaIntegration(
      "SingleConcurrencyIntegraiton",
      singleConcurrencyLambda
    );

    // By default, all routes go through the single concurrency integration
    const httpApi = new HttpApi(this, "HttpApi", {
      defaultIntegration: singleConcurrencyIntegraiton,
    });

    // This lambda is used for some whitelisted read / search / query / scroll endpoints
    const maxConcurrencyLambda = new Function(this, "MaxConcurrencyLambda", {
      ...commonLambdaParams,
      code: Code.fromAsset("../target/lambda/main_lambda/bootstrap.zip"),
      filesystem: LambdaFilesystem.fromEfsAccessPoint(accessPoint, "/mnt/efs"),
      vpc: vpc,
    });

    const maxConcurrencyIntegraiton = new HttpLambdaIntegration(
      "MaxConcurrencyIntegraiton",
      maxConcurrencyLambda
    );

    // Write routes go to the write integration.
    maxConcurrencyEndpoints.forEach((endpoint) => {
      new HttpRoute(this, endpoint.name, {
        httpApi: httpApi,
        routeKey: HttpRouteKey.with(endpoint.route, endpoint.method),
        integration: maxConcurrencyIntegraiton,
      });
    });

    new CfnOutput(this, "ApiGatewayURL", {
      value: httpApi.url!,
      exportName: "ApiGatewayURL",
    });
  }
}
