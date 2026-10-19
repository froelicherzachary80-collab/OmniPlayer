// Integration-test server for OmniCore.
//
// Minestom is an independent, open-source implementation of the Minecraft Java
// Edition 26.3 server protocol. It is used here only as a *test counterpart*
// so OmniCore can be exercised over a real TCP connection in environments where
// Mojang's server jar cannot be downloaded. OmniCore itself never depends on it,
// and the server manager runs the official vanilla/Paper jars for real play.
plugins {
    application
}

repositories {
    mavenLocal()
    mavenCentral()
}

dependencies {
    implementation("net.minestom:minestom:dev")
    implementation("org.slf4j:slf4j-simple:2.0.20")
}

java {
    toolchain { languageVersion.set(JavaLanguageVersion.of(25)) }
}

application {
    mainClass.set("omniplayer.testserver.TestServer")
}
