ExecutionEngine.class: ExecutionEngine.java
	javac ExecutionEngine.java


ExecutionEngine.h: ExecutionEngine.java
	javac -h . ExecutionEngine.java

ExecutionEngine.jar: ExecutionEngine.class
	jar -cf ExecutionEngine.jar ExecutionEngine.class

.PHONY: all run

all: ExecutionEngine.jar

clean:
	rm -f *.class *.jar

lib:
	cargo build -p casperlabs-engine-grpc-server

run: lib ExecutionEngine.h
	javac ExecutionEngine.java && java -Djava.library.path=target/debug/ ExecutionEngine
