import "./network";
import "./db";
import "./storage";
import "./cognito";
import "./websocket";
import "./ecr";
import "./lambda";

import { vpc, privateSubnetIds } from "./network";
import { rdsAddress, rdsEndpoint } from "./db";
import { mediaBucket, releasesBucket } from "./storage";
import { userPoolId, userPoolClientId, oauthDomain } from "./cognito";

export const vpcId = vpc.id;
export { privateSubnetIds, rdsAddress, rdsEndpoint, mediaBucket, releasesBucket, userPoolId, userPoolClientId, oauthDomain };
export { apiUrl, wsUrl } from "./lambda";
