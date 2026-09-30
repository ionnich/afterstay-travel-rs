FROM public.ecr.aws/lambda/provided:al2023
ARG CRATE=api
COPY ./target/lambda/${CRATE}/bootstrap /var/runtime/bootstrap
CMD ["bootstrap"]
