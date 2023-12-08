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
import { HttpApi } from "aws-cdk-lib/aws-apigatewayv2";
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

    const qdrantLambda = new Function(this, "QdrantLambda", {
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

    // HTTP API Gateway
    const httpApi = new HttpApi(this, "QdrantHttpApi", {
      defaultIntegration: new HttpLambdaIntegration(
        "DefaultIntegration",
        qdrantLambda
      ),
    });

    new CfnOutput(this, "ApiGatewayURL", {
      value: httpApi.url!,
      exportName: "ApiGatewayURL",
    });
  }
}
