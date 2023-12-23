import { Construct } from "constructs";
import { HttpLambdaIntegration } from "aws-cdk-lib/aws-apigatewayv2-integrations";

import {
  Architecture,
  Code,
  Function,
  Runtime,
  FileSystem as LambdaFilesystem,
} from "aws-cdk-lib/aws-lambda";
import { FileSystem, AccessPoint } from "aws-cdk-lib/aws-efs";

import { RetentionDays } from "aws-cdk-lib/aws-logs";
import {
  Stack,
  StackProps,
  Duration,
  App,
  RemovalPolicy,
  CfnOutput,
} from "aws-cdk-lib";
import {
  HttpApi,
  HttpRoute,
  HttpRouteKey,
  HttpMethod,
} from "aws-cdk-lib/aws-apigatewayv2";
import { Vpc } from "aws-cdk-lib/aws-ec2";
import { Lambda } from "aws-cdk-lib/aws-ses-actions";

export class QdrantLambdaStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    const vpc = new Vpc(this, "LambdaVpc", { natGateways: 0 });

    // Create a new EFS filesystem
    const fileSystem = new FileSystem(this, "LambdaEfs", {
      vpc,
      removalPolicy: RemovalPolicy.DESTROY, // adjust this as needed
    });

    const accessPoint = new AccessPoint(this, "AccessPoint", {
      fileSystem,
      path: "/export/lambda", // adjust this as needed
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

    // We're using separate lambdas for read and write
    // Ensures that in case we have multiple parallel writes,
    // We don't run into race conditions on the network file system
    // The write paths have a forced concurrency of 1.

    // TODO: Use separate qdrant configs for the read + write lambdas
    // because the read lambdas can be run with indexing memory of 0.

    const qdrantReadLambda = new Function(this, "QdrantReadLambda", {
      runtime: Runtime.PROVIDED_AL2023,
      handler: "not.required",
      architecture: Architecture.ARM_64,
      timeout: Duration.seconds(30),
      logRetention: RetentionDays.ONE_MONTH,
      memorySize: 3000, // 3 GB memory
      code: Code.fromAsset("../target/lambda/main_lambda/bootstrap.zip"),
      filesystem: LambdaFilesystem.fromEfsAccessPoint(accessPoint, "/mnt/efs"),
      vpc: vpc,
    });

    const qdrantWriteLambda = new Function(this, "QdrantWriteLambda", {
      runtime: Runtime.PROVIDED_AL2023,
      handler: "not.required",
      architecture: Architecture.ARM_64,
      timeout: Duration.seconds(30),
      logRetention: RetentionDays.ONE_MONTH,
      memorySize: 3000, // 3 GB memory
      //reservedConcurrentExecutions: 1, // Write lambda has a forced concurrency of 1, TODO, uncomment it once the concurrency limit is increased.
      code: Code.fromAsset("../target/lambda/main_lambda/bootstrap.zip"),
      filesystem: LambdaFilesystem.fromEfsAccessPoint(accessPoint, "/mnt/efs"),
      vpc: vpc,
    });

    const readIntegration = new HttpLambdaIntegration(
      "ReadIntegration",
      qdrantReadLambda
    );

    const writeIntegration = new HttpLambdaIntegration(
      "WriteIntegration",
      qdrantWriteLambda
    );

    // By default, all routes go through the read integration
    // AKA the lambda instance with full concurrency
    const httpApi = new HttpApi(this, "QdrantHttpApi", {
      defaultIntegration: readIntegration,
    });

    // Write routes should go to the write lambda,
    // Right now, just aded the put points, path
    // TODO: Add more paths
    new HttpRoute(this, "WriteRoutes", {
      httpApi: httpApi,
      routeKey: HttpRouteKey.with(
        "/collections/{collection_name}/points",
        HttpMethod.PUT
      ),
      integration: writeIntegration,
    });

    new CfnOutput(this, "ApiGatewayURL", {
      value: httpApi.url!,
      exportName: "ApiGatewayURL",
    });
  }
}
