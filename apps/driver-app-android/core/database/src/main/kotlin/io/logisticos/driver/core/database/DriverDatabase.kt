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
    version = 6,
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
