package omniplayer.testserver;

import net.kyori.adventure.text.Component;
import net.minestom.server.MinecraftServer;
import net.minestom.server.Auth;
import net.minestom.server.coordinate.Pos;
import net.minestom.server.coordinate.Vec;
import net.minestom.server.entity.Entity;
import net.minestom.server.entity.EntityCreature;
import net.minestom.server.entity.EntityType;
import net.minestom.server.entity.GameMode;
import net.minestom.server.entity.ItemEntity;
import net.minestom.server.entity.LivingEntity;
import net.minestom.server.entity.Player;
import net.minestom.server.entity.damage.Damage;
import net.minestom.server.event.entity.EntityAttackEvent;
import net.minestom.server.event.item.PickupItemEvent;
import net.minestom.server.event.player.*;
import net.minestom.server.instance.InstanceContainer;
import net.minestom.server.instance.block.Block;
import net.minestom.server.item.ItemStack;
import net.minestom.server.item.Material;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.time.Duration;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.stream.Collectors;

/**
 * Offline-mode survival test server. Every gameplay event the server accepts is
 * printed as a "[TEST] ..." line so integration tests can assert on what the
 * *server* saw, not on what the client claims.
 */
public final class TestServer {
    private static final Map<String, Long> lastMoveLog = new ConcurrentHashMap<>();
    private static InstanceContainer world;

    static void log(String s) {
        System.out.println("[TEST] " + s);
        System.out.flush();
    }

    public static void main(String[] args) {
        int port = args.length > 0 ? Integer.parseInt(args[0]) : 25565;
        MinecraftServer.setCompressionThreshold(256);
        MinecraftServer server = MinecraftServer.init(new Auth.Offline());

        world = MinecraftServer.getInstanceManager().createInstanceContainer();
        world.setGenerator(unit -> {
            unit.modifier().fillHeight(-64, -63, Block.BEDROCK);
            unit.modifier().fillHeight(-63, 55, Block.STONE);
            unit.modifier().fillHeight(55, 59, Block.DIRT);
            unit.modifier().fillHeight(59, 60, Block.GRASS_BLOCK);
        });
        // Load spawn chunks and add a tree, some coal ore and a cow pen.
        for (int cx = -2; cx <= 2; cx++) for (int cz = -2; cz <= 2; cz++) world.loadChunk(cx, cz).join();
        tree(6, 60, 6);
        tree(-7, 60, 4);
        for (int i = 0; i < 4; i++) world.setBlock(3 + i, 59, -4, Block.COAL_ORE);

        var events = MinecraftServer.getGlobalEventHandler();
        events.addListener(AsyncPlayerConfigurationEvent.class, e -> {
            e.setSpawningInstance(world);
            e.getPlayer().setRespawnPoint(new Pos(0.5, 60, 0.5));
        });
        events.addListener(PlayerSpawnEvent.class, e -> {
            Player p = e.getPlayer();
            p.setGameMode(GameMode.SURVIVAL);
            log("JOIN " + p.getUsername() + " " + p.getUuid());
        });
        events.addListener(PlayerDisconnectEvent.class, e -> log("LEAVE " + e.getPlayer().getUsername()));
        events.addListener(PlayerChatEvent.class, e -> log("CHAT " + e.getPlayer().getUsername() + " " + e.getRawMessage()));
        events.addListener(PlayerCommandEvent.class, e -> log("COMMAND " + e.getPlayer().getUsername() + " " + e.getCommand()));
        events.addListener(PlayerMoveEvent.class, e -> {
            long now = System.currentTimeMillis();
            String n = e.getPlayer().getUsername();
            if (now - lastMoveLog.getOrDefault(n, 0L) > 500) {
                lastMoveLog.put(n, now);
                Pos p = e.getNewPosition();
                log(String.format("MOVE %s %.3f %.3f %.3f %.1f %.1f %s", n, p.x(), p.y(), p.z(), p.yaw(), p.pitch(), e.isOnGround()));
            }
        });
        events.addListener(PlayerHandAnimationEvent.class, e -> log("SWING " + e.getPlayer().getUsername()));
        events.addListener(PlayerBlockBreakEvent.class, e -> {
            var pos = e.getBlockPosition();
            log("BREAK " + e.getPlayer().getUsername() + " " + pos.blockX() + " " + pos.blockY() + " " + pos.blockZ() + " " + e.getBlock().key().value());
            Material m = e.getBlock().registry().material();
            if (m != null && m != Material.AIR) {
                ItemEntity item = new ItemEntity(ItemStack.of(m));
                item.setPickupDelay(Duration.ofMillis(300));
                item.setInstance(e.getInstance(), new Pos(pos.blockX() + 0.5, pos.blockY() + 0.25, pos.blockZ() + 0.5));
            }
        });
        events.addListener(PlayerBlockPlaceEvent.class, e -> {
            var pos = e.getBlockPosition();
            log("PLACE " + e.getPlayer().getUsername() + " " + pos.blockX() + " " + pos.blockY() + " " + pos.blockZ() + " " + e.getBlock().key().value());
        });
        events.addListener(PickupItemEvent.class, e -> {
            if (e.getLivingEntity() instanceof Player p) {
                ItemStack s = e.getItemEntity().getItemStack();
                boolean ok = p.getInventory().addItemStack(s);
                e.setCancelled(!ok);
                if (ok) log("PICKUP " + p.getUsername() + " " + s.material().key().value() + " " + s.amount());
            }
        });
        events.addListener(EntityAttackEvent.class, e -> {
            Entity src = e.getEntity();
            Entity target = e.getTarget();
            String name = src instanceof Player p ? p.getUsername() : src.getEntityType().key().value();
            log("ATTACK " + name + " " + target.getEntityType().key().value() + " " + target.getEntityId());
            if (target instanceof LivingEntity living) {
                living.damage(Damage.fromEntity(src, 4));
                if (living.isDead()) log("KILLED " + name + " " + target.getEntityType().key().value());
            }
        });

        Thread console = new Thread(TestServer::console, "test-console");
        console.setDaemon(true);
        console.start();

        server.start("0.0.0.0", port);
        log("READY " + port + " protocol=" + MinecraftServer.PROTOCOL_VERSION + " version=" + MinecraftServer.VERSION_NAME);
    }

    static void tree(int x, int y, int z) {
        for (int i = 0; i < 5; i++) world.setBlock(x, y + i, z, Block.OAK_LOG);
        for (int dx = -2; dx <= 2; dx++) for (int dz = -2; dz <= 2; dz++) for (int dy = 3; dy <= 5; dy++) {
            if ((dx != 0 || dz != 0 || dy == 5) && Math.abs(dx) + Math.abs(dz) < 4)
                world.setBlock(x + dx, y + dy, z + dz, Block.OAK_LEAVES);
        }
    }

    /** Console commands used by the integration tests. */
    static void console() {
        try (BufferedReader in = new BufferedReader(new InputStreamReader(System.in))) {
            String line;
            while ((line = in.readLine()) != null) {
                String[] a = line.trim().split(" ");
                if (a.length == 0) continue;
                switch (a[0]) {
                    case "list" -> log("LIST " + MinecraftServer.getConnectionManager().getOnlinePlayers().stream()
                            .map(Player::getUsername).sorted().collect(Collectors.joining(",")));
                    case "say" -> {
                        String msg = line.substring(4);
                        MinecraftServer.getConnectionManager().getOnlinePlayers().forEach(p -> p.sendMessage(Component.text(msg)));
                        log("SAID " + msg);
                    }
                    case "block" -> {
                        int x = Integer.parseInt(a[1]), y = Integer.parseInt(a[2]), z = Integer.parseInt(a[3]);
                        log("BLOCK " + x + " " + y + " " + z + " " + world.getBlock(x, y, z).key().value());
                    }
                    case "pos" -> MinecraftServer.getConnectionManager().getOnlinePlayers().stream()
                            .filter(p -> p.getUsername().equals(a[1])).findFirst()
                            .ifPresent(p -> log(String.format("POS %s %.3f %.3f %.3f %.1f %.1f", a[1], p.getPosition().x(), p.getPosition().y(), p.getPosition().z(), p.getPosition().yaw(), p.getPosition().pitch())));
                    case "give" -> MinecraftServer.getConnectionManager().getOnlinePlayers().stream()
                            .filter(p -> p.getUsername().equals(a[1])).findFirst()
                            .ifPresent(p -> {
                                p.getInventory().addItemStack(ItemStack.of(Material.fromKey(a[2]), Integer.parseInt(a[3])));
                                log("GAVE " + a[1] + " " + a[2] + " " + a[3]);
                            });
                    case "zombie" -> {
                        EntityCreature z = new EntityCreature(EntityType.ZOMBIE);
                        z.setInstance(world, new Pos(Double.parseDouble(a[1]), Double.parseDouble(a[2]), Double.parseDouble(a[3]))).join();
                        log("SPAWNED zombie " + z.getEntityId());
                    }
                    case "inv" -> MinecraftServer.getConnectionManager().getOnlinePlayers().stream()
                            .filter(p -> p.getUsername().equals(a[1])).findFirst()
                            .ifPresent(p -> {
                                StringBuilder sb = new StringBuilder();
                                for (ItemStack s : p.getInventory().getItemStacks())
                                    if (!s.isAir()) sb.append(s.material().key().value()).append(':').append(s.amount()).append(',');
                                log("INV " + a[1] + " " + sb);
                            });
                    case "kick" -> MinecraftServer.getConnectionManager().getOnlinePlayers().stream()
                            .filter(p -> p.getUsername().equals(a[1])).findFirst()
                            .ifPresent(p -> p.kick(Component.text("Kicked by test")));
                    case "stop" -> {
                        log("STOPPING");
                        MinecraftServer.stopCleanly();
                        System.exit(0);
                    }
                    default -> log("UNKNOWN " + line);
                }
            }
        } catch (Exception ex) {
            log("CONSOLE_ERROR " + ex);
        }
    }
}
