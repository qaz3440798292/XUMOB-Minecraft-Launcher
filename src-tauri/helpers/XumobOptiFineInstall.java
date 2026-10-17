import java.io.File;
import java.io.IOException;

/** Invoke only against a launcher-owned staging root, never a live game root. */
public final class XumobOptiFineInstall {
    public static void main(String[] args) {
        System.setProperty("java.awt.headless", "true");
        try {
            if (args.length != 2 || !args[1].matches("[A-Za-z0-9][A-Za-z0-9._-]*")) {
                throw new IllegalArgumentException("Usage: XumobOptiFineInstall <staging-root> <minecraft-id>");
            }
            File root = new File(args[0]).getCanonicalFile();
            String[] declared = optifine.Installer.getOptiFineVersion().split("_");
            if (declared.length < 3 || !args[1].equals(declared[1])) {
                throw new IllegalArgumentException("Installer Minecraft version does not match staging input");
            }
            File base = new File(new File(root, "versions"), args[1]);
            requireFile(new File(base, args[1] + ".jar"));
            requireFile(new File(base, args[1] + ".json"));
            // Caller creates {"profiles":{}} in staging, without real accounts.
            requireFile(new File(root, "launcher_profiles.json"));
            optifine.Installer.doInstall(root);
            System.out.println("XUMOB_OPTIFINE_INSTALL_OK");
        } catch (Throwable error) {
            System.err.println("XUMOB_OPTIFINE_INSTALL_FAILED: " + error.getClass().getName());
            error.printStackTrace(System.err);
            System.exit(1);
        }
    }
    private static void requireFile(File file) throws IOException {
        if (!file.isFile()) throw new IOException("Missing staging input: " + file.getPath());
    }
}
