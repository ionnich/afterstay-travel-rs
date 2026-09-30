import * as aws from "@pulumi/aws";

const api = new aws.ecr.Repository("afterstay/api", {
  name: "afterstay/api",
  imageTagMutability: "MUTABLE",
  forceDelete: true,
});

const integrations = new aws.ecr.Repository("afterstay/integrations", {
  name: "afterstay/integrations",
  imageTagMutability: "MUTABLE",
  forceDelete: true,
});

const chat = new aws.ecr.Repository("afterstay/chat", {
  name: "afterstay/chat",
  imageTagMutability: "MUTABLE",
  forceDelete: true,
});

export const ecrRepos = {
  api: api.repositoryUrl,
  integrations: integrations.repositoryUrl,
  chat: chat.repositoryUrl,
};
