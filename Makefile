.PHONY: doctor build push deploy test clippy fmt e2e ci preview up destroy

doctor:  ; ./oracle.sh doctor
build:   ; ./oracle.sh build
push:    ; ./oracle.sh push
deploy:  ; ./oracle.sh deploy
test:    ; ./oracle.sh test
clippy:  ; ./oracle.sh clippy
fmt:     ; ./oracle.sh fmt
e2e:     ; ./oracle.sh e2e
ci:      ; ./oracle.sh ci
preview: ; ./oracle.sh preview
up:      ; ./oracle.sh up
destroy: ; ./oracle.sh destroy
