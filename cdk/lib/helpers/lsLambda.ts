import { Handler, Context } from "aws-lambda";
import { exec } from "child_process";
import { promisify } from "util";

const execPromise = promisify(exec);
const handler: Handler = async (event: any, context: Context) => {
  try {
    const { stdout } = await execPromise("ls -R /mnt/efs");
    const lines = stdout.split("\n");
    const result = {};
    let currentDir = result;
    const dirStack = [];

    for (const line of lines) {
      if (line.endsWith(":")) {
        const dir = line.slice(0, -1);
        const dirParts = dir.split("/");
        // Calculate the depth of the current directory
        const depth = dirParts.length - 1;
        // Adjust the currentDir to point to the correct parent directory
        while (dirStack.length > depth) {
          dirStack.pop();
        }
        const lastDirPart = dirParts[dirParts.length - 1];
        // If we are at the root, reset currentDir to result
        if (dirStack.length === 0) {
          currentDir = result;
        } else {
          currentDir = dirStack[dirStack.length - 1];
        }
        // Create a new directory in the currentDir and update the stack
        currentDir[lastDirPart] = {};
        currentDir = currentDir[lastDirPart];
        dirStack.push(currentDir);
      } else if (line) {
        // Add files to the current directory
        currentDir[line] = null;
      }
    }

    return JSON.stringify(result, null, 2);
  } catch (error) {
    console.error(error);
    throw error;
  }
};
export { handler };
