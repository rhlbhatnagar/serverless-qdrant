import { Construct } from "constructs";
import { HttpLambdaIntegration } from "aws-cdk-lib/aws-apigatewayv2-integrations";

import {
  Code,
  Function,
  FileSystem as LambdaFilesystem,
} from "aws-cdk-lib/aws-lambda";
import { FileSystem, AccessPoint } from "aws-cdk-lib/aws-efs";

import {
  Stack,
  StackProps,
  RemovalPolicy,
  CfnOutput,
  Size,
  Duration,
} from "aws-cdk-lib";
import {
  HttpApi,
  HttpMethod,
  HttpRoute,
  HttpRouteKey,
} from "aws-cdk-lib/aws-apigatewayv2";
import {
  GatewayVpcEndpointAwsService,
  InterfaceVpcEndpointAwsService,
  Vpc,
} from "aws-cdk-lib/aws-ec2";
import { commonLambdaParams, maxConcurrencyEndpoints } from "./config";
import { NodejsFunction } from "aws-cdk-lib/aws-lambda-nodejs";
import { Bucket } from "aws-cdk-lib/aws-s3";

export class QdrantLambdaStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    const vpc = new Vpc(this, "LambdaVpc", { natGateways: 0 });

    vpc.addGatewayEndpoint("Route53ResolverEndpoint", {
      service: GatewayVpcEndpointAwsService.S3,
    });

    vpc.addInterfaceEndpoint("VpcS3Endpoint", {
      service: InterfaceVpcEndpointAwsService.S3,
    });

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

    const lsLambda = new NodejsFunction(this, "LsLambda", {
      entry: "./lib/helpers/lsLambda.ts",
      handler: "handler",
      filesystem: LambdaFilesystem.fromEfsAccessPoint(accessPoint, "/mnt/efs"),
      vpc: vpc,
    });

    const bucket = new Bucket(this, "S3Bucket", {
      removalPolicy: RemovalPolicy.DESTROY,
    });

    const downloadS3Lambda = new Function(this, "DownloadS3Lambda", {
      ...commonLambdaParams,
      memorySize: 3000,
      timeout: Duration.minutes(10),
      ephemeralStorageSize: Size.mebibytes(10240),
      code: Code.fromAsset("../target/lambda/download_s3/bootstrap.zip"),
      filesystem: LambdaFilesystem.fromEfsAccessPoint(accessPoint, "/mnt/efs"),
      vpc: vpc,
    });

    bucket.grantRead(downloadS3Lambda);

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
        // Used for all endpoints by default, but should definitely be used for endpoints that write to the file system,
        // like upsert points, create collection etc.
        reservedConcurrentExecutions: 1,
        code: Code.fromAsset("../target/lambda/main_lambda/bootstrap.zip"),
        filesystem: LambdaFilesystem.fromEfsAccessPoint(
          accessPoint,
          "/mnt/efs"
        ),
        vpc: vpc,
      }
    );
    bucket.grantReadWrite(singleConcurrencyLambda);

    const singleConcurrencyIntegraiton = new HttpLambdaIntegration(
      "SingleConcurrencyIntegraiton",
      singleConcurrencyLambda
    );

    // By default, all routes go through the single concurrency integration
    const httpApi = new HttpApi(this, "HttpApi", {
      defaultIntegration: singleConcurrencyIntegraiton,
    });

    // This lambda is used for whitelisted read / search / query / scroll endpoints
    // basically ones that won't write to the file system, so that we can have multiple
    // concurrent access.
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

    const lsLambdaIntegration = new HttpLambdaIntegration(
      "LsLambdaIntegration",
      lsLambda
    );

    new HttpRoute(this, "LsLambdaRoute", {
      httpApi: httpApi,
      routeKey: HttpRouteKey.with("/lsLambda", HttpMethod.GET), // replace with your desired path and method
      integration: lsLambdaIntegration,
    });

    new CfnOutput(this, "ApiGatewayURL", {
      value: httpApi.url!,
      exportName: "ApiGatewayURL",
    });
  }
}
