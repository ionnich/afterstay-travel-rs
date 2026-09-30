import "./network";
import "./db";
import "./storage";
import "./cognito";
import "./websocket";
import "./ecr";

import { vpc, privateSubnetIds } from "./network";
import { rdsAddress, rdsEndpoint } from "./db";
import { mediaBucket } from "./storage";
import { userPoolId, userPoolClientId } from "./cognito";

export const vpcId = vpc.id;
export { privateSubnetIds, rdsAddress, rdsEndpoint, mediaBucket, userPoolId, userPoolClientId };
