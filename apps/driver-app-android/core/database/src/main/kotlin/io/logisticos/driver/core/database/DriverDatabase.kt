package io.logisticos.driver.core.database

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.TypeConverters
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import io.logisticos.driver.core.database.dao.*
import io.logisticos.driver.core.database.entity.*

val MIGRATION_3_4 = object : Migration(3, 4) {
    override fun migrate(database: SupportSQLiteDatabase) {
        database.execSQL(
            "ALTER TABLE tasks ADD COLUMN isSynced INTEGER NOT NULL DEFAULT 1"
        )
    }
}

val MIGRATION_4_5 = object : Migration(4, 5) {
    override fun migrate(database: SupportSQLiteDatabase) {
        database.execSQL(
            """
            CREATE TABLE IF NOT EXISTS notifications (
                id TEXT NOT NULL PRIMARY KEY,
                type TEXT NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                receivedAt INTEGER NOT NULL,
                isRead INTEGER NOT NULL DEFAULT 0
            )
            """.trimIndent()
        )
    }
}

val MIGRATION_5_6 = object : Migration(5, 6) {
    override fun migrate(database: SupportSQLiteDatabase) {
        database.execSQL("ALTER TABLE tasks ADD COLUMN podId TEXT")
        database.execSQL("ALTER TABLE tasks ADD COLUMN completedAt INTEGER")
    }
}

val MIGRATION_6_7 = object : Migration(6, 7) {
    override fun migrate(database: SupportSQLiteDatabase) {
        database.execSQL("ALTER TABLE tasks ADD COLUMN pop_id TEXT")
    }
}

/**
 * Adds the capture GPS and device-clock columns to `pod`.
 *
 * All three are nullable with no default: rows written before this migration
 * genuinely have no capture position or device timestamp, and inventing one
 * (e.g. defaulting to 0.0) would be worse than sending nothing — the server
 * treats absent values as "older client" and falls back to its own clock,
 * whereas 0.0/0.0 is a real coordinate in the Gulf of Guinea.
 */
val MIGRATION_7_8 = object : Migration(7, 8) {
    override fun migrate(database: SupportSQLiteDatabase) {
        database.execSQL("ALTER TABLE pod ADD COLUMN capture_lat REAL")
        database.execSQL("ALTER TABLE pod ADD COLUMN capture_lng REAL")
        database.execSQL("ALTER TABLE pod ADD COLUMN device_timestamp TEXT")
    }
}

@TypeConverters(Converters::class)
@Database(
    entities = [
        ShiftEntity::class,
        TaskEntity::class,
        RouteEntity::class,
        PodEntity::class,
        LocationBreadcrumbEntity::class,
        ScanEventEntity::class,
        SyncQueueEntity::class,
        NotificationEntity::class,
    ],
    version = 8,
    exportSchema = true
)
abstract class DriverDatabase : RoomDatabase() {
    abstract fun shiftDao(): ShiftDao
    abstract fun taskDao(): TaskDao
    abstract fun routeDao(): RouteDao
    abstract fun podDao(): PodDao
    abstract fun locationBreadcrumbDao(): LocationBreadcrumbDao
    abstract fun scanEventDao(): ScanEventDao
    abstract fun syncQueueDao(): SyncQueueDao
    abstract fun notificationDao(): NotificationDao
}
